// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{Mutation, RawTxnOp, RawTxnResponse};

impl Service {
    async fn mutate(
        &self,
        store: Arc<dyn MetaStore>,
        key: Vec<u8>,
        mutation: Mutation,
        previous: bool,
    ) -> Result<RawTxnResponse, StoreError> {
        let mut ops = Vec::with_capacity(if previous { 2 } else { 1 });
        if previous {
            ops.push(RawTxnOp::Get(key));
        }
        ops.push(RawTxnOp::Write(mutation));
        tokio::select! { biased;
            _ = self.cancel.cancelled() => Err(StoreError::Cancelled),
            response = store.transact_raw(vec![], ops, vec![]) => response,
        }
    }
    pub(crate) async fn put_request(
        &self,
        request: p::PutRequest,
    ) -> Result<p::PutResponse, Status> {
        let store = self.store().await?;
        let invalid = if request.key.is_empty() {
            Some("etcdserver: key is not provided".into())
        } else if request.lease < 0 {
            Some(error_message(StoreError::LeaseNotFound))
        } else {
            None
        };
        if invalid.is_some() {
            return Ok(p::PutResponse {
                header: self.header(0, invalid),
                ..Default::default()
            });
        }
        let result = self
            .mutate(
                store,
                request.key.clone(),
                Mutation::Put {
                    key: request.key,
                    value: request.value,
                    lease: (request.lease != 0).then_some(request.lease),
                },
                request.prev_kv,
            )
            .await;
        Ok(match result {
            Ok(response) => p::PutResponse {
                header: self.header(response.revision, None),
                prev_kv: if request.prev_kv {
                    previous(&response).into_iter().next()
                } else {
                    None
                },
            },
            Err(error) => p::PutResponse {
                header: self.header(0, Some(error_message(error))),
                ..Default::default()
            },
        })
    }
    pub(crate) async fn delete_request(
        &self,
        request: p::DeleteRequest,
    ) -> Result<p::DeleteResponse, Status> {
        let store = self.store().await?;
        if request.key.is_empty() {
            return Ok(p::DeleteResponse {
                header: self.header(0, Some("etcdserver: key is not provided".into())),
                ..Default::default()
            });
        }
        // The Go service deliberately does not forward range_end or populate deleted.
        let result = self
            .mutate(
                store,
                request.key.clone(),
                Mutation::Delete { key: request.key },
                request.prev_kv,
            )
            .await;
        Ok(match result {
            Ok(response) => p::DeleteResponse {
                header: self.header(response.revision, None),
                prev_kvs: if request.prev_kv {
                    previous(&response)
                } else {
                    vec![]
                },
                deleted: 0,
            },
            Err(error) => p::DeleteResponse {
                header: self.header(0, Some(error_message(error))),
                ..Default::default()
            },
        })
    }
}
fn previous(response: &RawTxnResponse) -> Vec<p::KeyValue> {
    response
        .responses
        .first()
        .into_iter()
        .flat_map(|item| &item.key_value_pairs)
        .map(|kv| p::KeyValue {
            key: kv.key.clone(),
            value: kv.value.clone(),
            ..Default::default()
        })
        .collect()
}
