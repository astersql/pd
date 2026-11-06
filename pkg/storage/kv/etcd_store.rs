// Copyright 2026 AsterSQL.
use crate::raw_get::validate_raw;
use crate::*;
use async_trait::async_trait;
use etcd_client::{
    Client, CompareOp, ConnectOptions, GetOptions, PutOptions, Txn, TxnOp, WatchOptions,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Transitional external etcd adapter, not the final embedded metadata engine.
/// Cloning the multiplexed client per operation avoids serializing independent RPCs.
#[derive(Clone)]
pub struct EtcdMetaStore {
    client: Client,
    timeout: Duration,
    endpoints: Vec<String>,
    health_clients: Option<Vec<Client>>,
}
impl EtcdMetaStore {
    fn new_watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
        with_previous: bool,
    ) -> Result<Box<dyn MetaWatch>> {
        if start_revision <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        Ok(Box::new(EtcdWatch {
            store: self.clone(),
            prefix,
            range_end: None,
            header_revision: None,
            next: start_revision,
            active: None,
            previous: Default::default(),
            with_previous,
            cancelled: false,
            progress_request: None,
        }))
    }

    pub async fn connect(endpoints: &[String], timeout: Duration) -> Result<Self> {
        if endpoints.is_empty() || timeout.is_zero() {
            return Err(StoreError::InvalidArgument);
        }
        let options = ConnectOptions::new()
            .with_connect_timeout(timeout)
            .with_require_leader(true);
        let client = tokio::time::timeout(timeout, Client::connect(endpoints, Some(options)))
            .await
            .map_err(|_| StoreError::Unavailable)?
            .map_err(map_error)?;
        Ok(Self {
            client,
            timeout,
            endpoints: endpoints.to_vec(),
            health_clients: None,
        })
    }
    /// Connect the unchanged native SDK through caller-owned transports. The
    /// factory also supplies isolated per-endpoint health paths with identical
    /// credentials; no transport types enter the backend-neutral MetaStore trait.
    pub async fn connect_with_transport_factory<S, F>(
        endpoints: &[String],
        timeout: Duration,
        factory: F,
    ) -> Result<Self>
    where
        S: tower::Service<
                tonic::codegen::http::Request<tonic::body::Body>,
                Response = tonic::codegen::http::Response<tonic::body::Body>,
            > + Clone
            + Send
            + Sync
            + 'static,
        S::Error: Into<tower::BoxError>,
        S::Future: Send + 'static,
        F: Fn(&[String]) -> Result<S>,
    {
        use tower::ServiceExt;
        if endpoints.is_empty() || timeout.is_zero() {
            return Err(StoreError::InvalidArgument);
        }
        let connect = |targets: &[String]| {
            let transport = factory(targets)?;
            let service = tower::util::BoxCloneSyncService::new(
                transport.map_err(Into::<tower::BoxError>::into),
            );
            Ok::<_, StoreError>(Client::from_channel(
                etcd_client::Channel::Custom(service),
                Some(
                    ConnectOptions::new()
                        .with_connect_timeout(timeout)
                        .with_require_leader(true),
                ),
            ))
        };
        let client = tokio::time::timeout(timeout, connect(endpoints)?)
            .await
            .map_err(|_| StoreError::Unavailable)?
            .map_err(map_error)?;
        let mut health_clients = Vec::with_capacity(endpoints.len());
        for endpoint in endpoints {
            let client = tokio::time::timeout(timeout, connect(std::slice::from_ref(endpoint))?)
                .await
                .map_err(|_| StoreError::Unavailable)?
                .map_err(map_error)?;
            health_clients.push(client);
        }
        Ok(Self {
            client,
            timeout,
            endpoints: endpoints.to_vec(),
            health_clients: Some(health_clients),
        })
    }
    async fn read_with_count(&self, r: Read) -> Result<(Snapshot, i64)> {
        r.validate()?;
        let start = r
            .start
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if r.prefix.is_empty() {
                    vec![0]
                } else {
                    r.prefix.clone()
                }
            });
        let end = r.end.clone().unwrap_or_else(|| prefix_end(&r.prefix));
        let mut options = GetOptions::new().with_range(end).with_limit(r.limit as i64);
        if r.consistency == Consistency::Serializable {
            options = options.with_serializable();
        }
        if let Some(rev) = r.revision {
            options = options.with_revision(rev);
        }
        let mut client = self.client.clone();
        let response = self
            .bounded(client.get(start, Some(options)))
            .await
            .map_err(|e| match e {
                StoreError::Compacted {
                    compact_revision, ..
                } => StoreError::Compacted {
                    requested: r.revision.unwrap_or(0),
                    compact_revision,
                },
                other => other,
            })?;
        Ok((
            Snapshot {
                revision: revision(response.header())?,
                entries: response.kvs().iter().map(entry).collect(),
                more: response.more(),
            },
            response.count(),
        ))
    }
    async fn bounded<T>(
        &self,
        future: impl std::future::Future<Output = std::result::Result<T, etcd_client::Error>>,
    ) -> Result<T> {
        tokio::time::timeout(self.timeout, future)
            .await
            .map_err(|_| StoreError::Unavailable)?
            .map_err(map_error)
    }
}
pub(crate) fn map_error(error: etcd_client::Error) -> StoreError {
    use etcd_client::Error;
    use tonic::Code;
    match error {
        Error::GRpcStatus(status) => match status.code() {
            Code::Cancelled => StoreError::Cancelled,
            Code::Unavailable | Code::DeadlineExceeded | Code::Unknown => StoreError::Unavailable,
            Code::PermissionDenied | Code::Unauthenticated => StoreError::PermissionDenied,
            Code::NotFound => StoreError::LeaseNotFound,
            // etcd's v3 protocol uses OutOfRange for both cases, without structured details.
            Code::OutOfRange
                if status.message() == "etcdserver: mvcc: required revision has been compacted" =>
            {
                StoreError::Compacted {
                    requested: 0,
                    compact_revision: None,
                }
            }
            Code::OutOfRange
                if status.message()
                    == "etcdserver: mvcc: required revision is a future revision" =>
            {
                StoreError::FutureRevision
            }
            Code::InvalidArgument if status.message() == "etcdserver: request is too large" => {
                StoreError::RequestTooLarge
            }
            Code::InvalidArgument => StoreError::InvalidArgument,
            _ => StoreError::Backend,
        },
        Error::InvalidArgs(_) | Error::InvalidUri(_) | Error::InvalidHeaderValue(_) => {
            StoreError::InvalidArgument
        }
        Error::TransportError(_) | Error::IoError(_) | Error::WatchError(_) => {
            StoreError::Unavailable
        }
        // etcd-client consumes the initial keepalive response and represents TTL=0 this way.
        Error::LeaseKeepAliveError(reason) if reason == "lease not found" => {
            StoreError::LeaseNotFound
        }
        Error::LeaseKeepAliveError(_) => StoreError::Unavailable,
        _ => StoreError::Backend,
    }
}
fn entry(kv: &etcd_client::KeyValue) -> Entry {
    Entry {
        version: kv.version(),
        key: kv.key().to_vec(),
        value: kv.value().to_vec(),
        create_revision: kv.create_revision(),
        mod_revision: kv.mod_revision(),
        lease: (kv.lease() != 0).then_some(kv.lease()),
    }
}
fn revision(header: Option<&etcd_client::ResponseHeader>) -> Result<Revision> {
    header.map(|h| h.revision()).ok_or(StoreError::Backend)
}
fn prefix_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    while let Some(byte) = end.pop() {
        if byte != 255 {
            end.push(byte + 1);
            return end;
        }
    }
    vec![0]
}
#[async_trait]
impl MetaStore for EtcdMetaStore {
    async fn backend_health(&self) -> Vec<BackendHealth> {
        let mut observations = Vec::with_capacity(self.endpoints.len());
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let begin = std::time::Instant::now();
            let response = tokio::time::timeout(self.timeout, async {
                let mut client = if let Some(clients) = &self.health_clients {
                    clients[index].clone()
                } else {
                    Client::connect(
                        [endpoint.clone()],
                        Some(
                            ConnectOptions::new()
                                .with_connect_timeout(self.timeout)
                                .with_require_leader(true),
                        ),
                    )
                    .await?
                };
                client.get("health", None).await
            })
            .await;
            let healthy = match response {
                Ok(Ok(_)) => true,
                Ok(Err(error)) => map_error(error) == StoreError::PermissionDenied,
                Err(_) => false,
            };
            observations.push(BackendHealth {
                endpoint: endpoint.clone(),
                healthy,
                elapsed: begin.elapsed(),
            });
        }
        observations
    }
    async fn consensus_cluster_id(&self) -> Result<Option<u64>> {
        let mut client = self.client.clone();
        let status = self.bounded(client.status()).await?;
        Ok(status.header().map(|header| header.cluster_id()))
    }
    async fn consensus_leader_id(&self) -> Result<Option<u64>> {
        let mut client = self.client.clone();
        let status = self.bounded(client.status()).await?;
        Ok((status.leader() != 0).then_some(status.leader()))
    }

    async fn read(&self, r: Read) -> Result<Snapshot> {
        self.read_with_count(r).await.map(|(snapshot, _)| snapshot)
    }
    async fn read_counted(&self, r: Read) -> Result<(Snapshot, i64)> {
        self.read_with_count(r).await
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        self.transact_branches(compares, writes, vec![]).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit> {
        let response = self
            .transact_raw(
                compares,
                then_writes.into_iter().map(Into::into).collect(),
                else_writes.into_iter().map(Into::into).collect(),
            )
            .await?;
        Ok(Commit {
            succeeded: response.succeeded,
            revision: response.revision,
        })
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        validate_raw(&compares, &then_ops)?;
        validate_raw(&compares, &else_ops)?;
        let compares: Vec<_> = compares
            .into_iter()
            .map(|c| match c {
                Compare::CreateRevision(k, r) => {
                    etcd_client::Compare::create_revision(k, CompareOp::Equal, r)
                }
                Compare::ModRevision(k, r) => {
                    etcd_client::Compare::mod_revision(k, CompareOp::Equal, r)
                }
                Compare::Value(k, v) => etcd_client::Compare::value(k, CompareOp::Equal, v),
                Compare::ValueNotEqual(k, v) => {
                    etcd_client::Compare::value(k, CompareOp::NotEqual, v)
                }
                Compare::ValueLess(k, v) => etcd_client::Compare::value(k, CompareOp::Less, v),
                Compare::ValueGreater(k, v) => {
                    etcd_client::Compare::value(k, CompareOp::Greater, v)
                }
                Compare::Exists(k) => {
                    etcd_client::Compare::create_revision(k, CompareOp::Greater, 0)
                }
                Compare::NotExists(k) => {
                    etcd_client::Compare::create_revision(k, CompareOp::Equal, 0)
                }
                Compare::Lease(k, id) => etcd_client::Compare::lease(k, CompareOp::Equal, id),
            })
            .collect();
        let operations = |ops: Vec<RawTxnOp>| -> Vec<TxnOp> {
            ops.into_iter()
                .map(|op| match op {
                    RawTxnOp::Get(key) => TxnOp::get(key, None),
                    RawTxnOp::GetRange { key, end, limit } => {
                        let read = crate::raw_range::request(&key, &end, limit);
                        let start = read.start.unwrap_or_else(|| {
                            if read.prefix.is_empty() {
                                vec![0]
                            } else {
                                read.prefix.clone()
                            }
                        });
                        let end = read.end.unwrap_or_else(|| prefix_end(&read.prefix));
                        TxnOp::get(
                            start,
                            Some(GetOptions::new().with_range(end).with_limit(limit as i64)),
                        )
                    }
                    RawTxnOp::Write(Mutation::Put { key, value, lease }) => {
                        TxnOp::put(key, value, lease.map(|id| PutOptions::new().with_lease(id)))
                    }
                    RawTxnOp::Write(Mutation::Delete { key }) => TxnOp::delete(key, None),
                })
                .collect()
        };
        let mut client = self.client.clone();
        let response = self
            .bounded(
                client.txn(
                    Txn::new()
                        .when(compares)
                        .and_then(operations(then_ops.clone()))
                        .or_else(operations(else_ops.clone())),
                ),
            )
            .await
            .map_err(crate::txn_cancel::dispatched_error)?;
        let ops = if response.succeeded() {
            then_ops
        } else {
            else_ops
        };
        decode_raw(
            response.succeeded(),
            revision(response.header())?,
            &ops,
            response.op_responses(),
        )
    }

    async fn watch(&self, prefix: Vec<u8>, start_revision: Revision) -> Result<Box<dyn MetaWatch>> {
        self.new_watch(prefix, start_revision, false)
    }
    async fn watch_with_previous(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> Result<Box<dyn MetaWatch>> {
        self.new_watch(prefix, start_revision, true)
    }
    async fn watch_range(
        &self,
        key: Vec<u8>,
        end: Vec<u8>,
        start_revision: Revision,
        previous: bool,
    ) -> Result<Box<dyn MetaWatch>> {
        if key.is_empty() {
            return Err(StoreError::InvalidArgument);
        }
        let start_revision = start_revision.max(0);
        Ok(Box::new(EtcdWatch {
            with_previous: previous,
            previous: Default::default(),
            store: self.clone(),
            prefix: key,
            range_end: Some(end),
            header_revision: None,
            next: start_revision,
            active: None,
            cancelled: false,
            progress_request: None,
        }))
    }
    async fn grant(&self, ttl_seconds: i64) -> Result<Lease> {
        if !(1..=i32::MAX as i64).contains(&ttl_seconds) {
            return Err(StoreError::InvalidArgument);
        }
        let mut client = self.client.clone();
        let r = self.bounded(client.lease_grant(ttl_seconds, None)).await?;
        Ok(Lease {
            id: r.id(),
            ttl_seconds: r.ttl(),
        })
    }
    async fn keep_alive(&self, id: LeaseId) -> Result<Lease> {
        let mut client = self.client.clone();
        let (mut keeper, mut stream) = self.bounded(client.lease_keep_alive(id)).await?;
        self.bounded(keeper.keep_alive()).await?;
        let response = self
            .bounded(stream.message())
            .await?
            .ok_or(StoreError::Unavailable)?;
        if response.ttl() <= 0 {
            return Err(StoreError::LeaseNotFound);
        }
        Ok(Lease {
            id: response.id(),
            ttl_seconds: response.ttl(),
        })
    }
    async fn revoke(&self, id: LeaseId) -> Result<()> {
        let mut client = self.client.clone();
        self.bounded(client.lease_revoke(id)).await?;
        Ok(())
    }
    async fn ttl(&self, id: LeaseId) -> Result<i64> {
        let mut client = self.client.clone();
        let r = self.bounded(client.lease_time_to_live(id, None)).await?;
        if r.ttl() < 0 {
            return Err(StoreError::LeaseNotFound);
        }
        Ok(r.ttl())
    }
    async fn compact(&self, rev: Revision) -> Result<()> {
        if rev <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        let mut client = self.client.clone();
        self.bounded(client.compact(rev, None)).await?;
        Ok(())
    }
}
struct EtcdWatch {
    header_revision: Option<Revision>,
    range_end: Option<Vec<u8>>,
    with_previous: bool,
    previous: std::collections::BTreeMap<(Revision, Vec<u8>), Vec<u8>>,
    store: EtcdMetaStore,
    prefix: Vec<u8>,
    next: Revision,
    active: Option<(etcd_client::Watcher, etcd_client::WatchStream)>,
    cancelled: bool,
    progress_request: Option<std::time::Instant>,
}
impl EtcdWatch {
    async fn receive(&mut self, progress: bool) -> Result<WatchBatch> {
        loop {
            if self.active.is_none() {
                let mut client = self.store.client.clone();
                let (key, options) = if let Some(end) = &self.range_end {
                    (
                        self.prefix.clone(),
                        WatchOptions::new().with_range(end.clone()),
                    )
                } else if self.prefix.is_empty() {
                    (vec![0], WatchOptions::new().with_all_keys())
                } else {
                    (self.prefix.clone(), WatchOptions::new().with_prefix())
                };
                let options = if self.with_previous {
                    options.with_prev_key()
                } else {
                    options
                };
                self.active = Some(
                    self.store
                        .bounded(
                            client.watch(
                                key,
                                Some(
                                    options
                                        .with_start_revision(self.next)
                                        .with_progress_notify(),
                                ),
                            ),
                        )
                        .await?,
                );
            }
            let wait = self
                .progress_request
                .get_or_insert_with(|| std::time::Instant::now() + Duration::from_secs(1))
                .saturating_duration_since(std::time::Instant::now());
            let active = self.active.as_mut().expect("watch established");
            let message = if progress {
                match tokio::time::timeout(wait, active.1.message()).await {
                    Ok(message) => message,
                    Err(_) => {
                        active.0.request_progress().await.map_err(map_error)?;
                        self.progress_request =
                            Some(std::time::Instant::now() + Duration::from_secs(1));
                        continue;
                    }
                }
            } else {
                active.1.message().await
            };
            let response = match message {
                Ok(Some(response)) => response,
                Ok(None) => {
                    self.active = None;
                    return Err(StoreError::Unavailable);
                }
                Err(error) => {
                    self.active = None;
                    return Err(map_error(error));
                }
            };
            if response.compact_revision() > 0 {
                self.active = None;
                return Err(StoreError::Compacted {
                    requested: self.next,
                    compact_revision: Some(response.compact_revision()),
                });
            }
            if response.canceled() {
                self.active = None;
                return Err(StoreError::Unavailable);
            }
            self.header_revision = response.header().map(|header| header.revision());
            self.previous.clear();
            for event in response.events() {
                if let (Some(kv), Some(previous)) = (event.kv(), event.prev_kv()) {
                    self.previous.insert(
                        (kv.mod_revision(), kv.key().to_vec()),
                        previous.value().to_vec(),
                    );
                }
            }
            let events: Vec<_> = response
                .events()
                .iter()
                .filter_map(|event| {
                    event.kv().map(|kv| match event.event_type() {
                        etcd_client::EventType::Put => Event::Put(entry(kv)),
                        etcd_client::EventType::Delete => Event::Delete {
                            key: kv.key().to_vec(),
                            revision: kv.mod_revision(),
                        },
                    })
                })
                .collect();
            if progress && !response.created() && events.is_empty() {
                let revision = revision(response.header())?;
                self.next = self.next.max(revision + 1);
                return Ok(WatchBatch { events, revision });
            }
            if let Some(last) = events.last() {
                // Created/progress headers may be ahead of replayed events; never use them as a cursor.
                let revision = last.revision();
                self.next = revision + 1;
                return Ok(WatchBatch { events, revision });
            }
        }
    }
}
#[async_trait]
impl MetaWatch for EtcdWatch {
    fn header_revision(&self) -> Option<Revision> {
        self.header_revision
    }
    async fn next_with_progress(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        if self.cancelled {
            return Err(StoreError::Cancelled);
        }
        tokio::select! { biased;
            _ = cancel.cancelled() => {self.active=None;self.cancelled=true;Err(StoreError::Cancelled)},
            result = self.receive(true) => result,
        }
    }

    fn previous_value(&self, event: &Event) -> Option<Vec<u8>> {
        self.previous
            .get(&(event.revision(), event.key().to_vec()))
            .cloned()
    }

    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        if self.cancelled {
            return Err(StoreError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                // Dropping both stream ends closes the RPC, including on a partitioned endpoint.
                self.active = None; self.cancelled = true; Err(StoreError::Cancelled)
            },
            result = self.receive(false) => result,
        }
    }
}

/// Validate the selected branch's response before exposing domain results.
pub(crate) fn decode_raw(
    succeeded: bool,
    revision: Revision,
    ops: &[RawTxnOp],
    rpc_items: Vec<etcd_client::TxnOpResponse>,
) -> Result<RawTxnResponse> {
    if rpc_items.len() != ops.len() {
        return Err(StoreError::Backend);
    }
    let mut responses = Vec::with_capacity(ops.len());
    for (op, response) in ops.iter().zip(rpc_items) {
        let item = match (op, response) {
            (RawTxnOp::Get(_) | RawTxnOp::GetRange { .. }, etcd_client::TxnOpResponse::Get(r)) => {
                RawTxnResponseItem {
                    more: r.more(),
                    key_value_pairs: r
                        .kvs()
                        .iter()
                        .map(|kv| KeyValuePair {
                            key: kv.key().to_vec(),
                            value: kv.value().to_vec(),
                        })
                        .collect(),
                }
            }
            (RawTxnOp::Write(Mutation::Put { .. }), etcd_client::TxnOpResponse::Put(_))
            | (RawTxnOp::Write(Mutation::Delete { .. }), etcd_client::TxnOpResponse::Delete(_)) => {
                RawTxnResponseItem::default()
            }
            _ => return Err(StoreError::Backend),
        };
        responses.push(item);
    }
    Ok(RawTxnResponse {
        succeeded,
        revision,
        responses,
    })
}

#[path = "etcd_membership.rs"]
mod membership;
