// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::pdpb::pd_client::PdClient;
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(super) struct ExternalStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) _fixture: std::sync::Mutex<crate::node_test::fixture::Fixture>,
    pub(super) mode: std::sync::atomic::AtomicU8,
    pub(super) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ExternalStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        if writes
            .iter()
            .any(|write| matches!(write, Mutation::Put {key, ..} if key.ends_with(b"/external_timestamp")))
        {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::put(b"/task035/oversized", &vec![0; 2 * 1024 * 1024])),
                2 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
        }
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> astersql_pd_storage::Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> astersql_pd_storage::Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> astersql_pd_storage::Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> astersql_pd_storage::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage::Result<()> {
        self.inner.compact(revision).await
    }
}

#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_external_ts_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_035_real_transaction_failure_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::node_test::fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(inner.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(3502);
    let backend = Arc::new(ExternalStore {
        inner,
        _fixture: std::sync::Mutex::new(fixture),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 3502,
                member_id: 1,
                name: "external-fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    prepare::prepare(&mut client, 3502).await;
    assert!(
        scenarios::set(&mut client, 3502, 100)
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    backend.mode.store(1, Ordering::SeqCst);
    let error = scenarios::set(&mut client, 3502, 101)
        .await
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(
        error.r#type,
        astersql_pd_proto::pdpb::ErrorType::InvalidValue as i32
    );
    assert_eq!(error.message, "unexpected metadata backend response");
    assert_eq!(scenarios::get(&mut client, 3502).await.timestamp, 101);
    assert_eq!(storage.load_external_ts(&paths).await.unwrap(), 100);
    assert!(
        scenarios::set(&mut client, 3502, 101)
            .await
            .header
            .unwrap()
            .error
            .is_some()
    );
    assert!(
        scenarios::set(&mut client, 3502, 102)
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(storage.load_external_ts(&paths).await.unwrap(), 102);
    backend.mode.store(2, Ordering::SeqCst);
    let pending = tokio::spawn(async move {
        client
            .set_external_timestamp(astersql_pd_proto::pdpb::SetExternalTimestampRequest {
                header: scenarios::header(3502),
                timestamp: 103,
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(storage.load_external_ts(&paths).await.unwrap(), 102);
    node.shutdown().await.unwrap();
}
