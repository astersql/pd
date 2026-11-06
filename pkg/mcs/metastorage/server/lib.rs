// Copyright 2026 AsterSQL.
//! MetaStorage protocol adapter over the host's canonical metadata backend.
use astersql_pd_proto::meta_storagepb::{self as p, meta_storage_server::MetaStorage};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read, StoreError};
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

/// Live admission and shared storage supplied by the host, without private runtime state.
#[async_trait]
pub trait Host: Send + Sync {
    fn cluster_id(&self) -> u64;
    fn store(&self) -> Option<Arc<dyn MetaStore>>;
    async fn is_serving(&self) -> bool;
}

/// One listener's adapter. Admission is checked for every request.
#[derive(Clone)]
pub struct Service {
    host: Arc<dyn Host>,
    cancel: CancellationToken,
}
impl Service {
    pub fn new(host: Arc<dyn Host>, cancel: CancellationToken) -> Self {
        Self { host, cancel }
    }
    async fn store(&self) -> Result<Arc<dyn MetaStore>, Status> {
        if self.cancel.is_cancelled() || !self.host.is_serving().await {
            return Err(Status::unavailable("not leader"));
        }
        self.host
            .store()
            .ok_or_else(|| Status::unavailable("not leader"))
    }
    fn header(&self, revision: i64, error: Option<String>) -> Option<p::ResponseHeader> {
        Some(p::ResponseHeader {
            cluster_id: self.host.cluster_id(),
            revision,
            error: error.map(|message| p::Error {
                r#type: p::ErrorType::Unknown as i32,
                message,
            }),
        })
    }
}

fn error_message(error: StoreError) -> String {
    match error {
        StoreError::Compacted { .. } => "etcdserver: mvcc: required revision has been compacted",
        StoreError::FutureRevision => "etcdserver: mvcc: required revision is a future revision",
        StoreError::LeaseNotFound => "etcdserver: requested lease not found",
        StoreError::PermissionDenied => "etcdserver: permission denied",
        StoreError::RequestTooLarge => "etcdserver: request is too large",
        StoreError::Cancelled => "context canceled",
        StoreError::Unavailable => "etcdserver: request timed out",
        other => return other.to_string(),
    }
    .into()
}

#[tonic::async_trait]
impl MetaStorage for Service {
    async fn watch(
        &self,
        request: Request<p::WatchRequest>,
    ) -> Result<Response<tonic::codegen::BoxStream<p::WatchResponse>>, Status> {
        self.watch_request(request.into_inner())
            .await
            .map(Response::new)
    }
    async fn put(
        &self,
        request: Request<p::PutRequest>,
    ) -> Result<Response<p::PutResponse>, Status> {
        self.put_request(request.into_inner())
            .await
            .map(Response::new)
    }
    async fn delete(
        &self,
        request: Request<p::DeleteRequest>,
    ) -> Result<Response<p::DeleteResponse>, Status> {
        self.delete_request(request.into_inner())
            .await
            .map(Response::new)
    }
    async fn get(
        &self,
        request: Request<p::GetRequest>,
    ) -> Result<Response<p::GetResponse>, Status> {
        let store = self.store().await?;
        let request = request.into_inner();
        if request.key.is_empty() {
            return Ok(Response::new(p::GetResponse {
                header: self.header(0, Some("etcdserver: key is not provided".into())),
                ..Default::default()
            }));
        }
        let mut read = if request.range_end == [0] {
            let mut read = Read::prefix(b"", Consistency::Linearizable);
            read.start = Some(request.key);
            read
        } else {
            Read::range(&request.key, &request.range_end, Consistency::Linearizable)
        };
        read.limit = usize::try_from(request.limit).unwrap_or(0);
        read.revision = (request.revision > 0).then_some(request.revision);
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => Err(StoreError::Cancelled),
            result = store.read_counted(read) => result,
        };
        let response = match result {
            Ok((snapshot, count)) => p::GetResponse {
                header: self.header(snapshot.revision, None),
                count,
                more: snapshot.more,
                kvs: snapshot
                    .entries
                    .into_iter()
                    .map(|entry| p::KeyValue {
                        key: entry.key,
                        value: entry.value,
                        ..Default::default()
                    })
                    .collect(),
            },
            Err(error) => p::GetResponse {
                header: self.header(0, Some(error_message(error))),
                ..Default::default()
            },
        };
        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod metastorage_get_test;
mod metastorage_watch;
#[cfg(test)]
mod metastorage_watch_test;
mod metastorage_write;
#[cfg(test)]
mod metastorage_write_test;
