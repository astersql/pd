// Copyright 2026 AsterSQL.
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
struct ClusterStore {
    inner: Arc<dyn MetaStore>,
    mode: std::sync::atomic::AtomicU8,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ClusterStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, mut request: Read) -> astersql_pd_storage::Result<Snapshot> {
        if request
            .start
            .as_ref()
            .is_some_and(|key| key.ends_with(b"/raft_bootstrap_time"))
        {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => request.revision = Some(i64::MAX),
                2 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
        }
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_046_storage_fault_and_cancel() {
    use crate::http_member_test::request;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let store = Arc::new(ClusterStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7461,
                member_id: 1,
                name: "cluster".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let read = Read::prefix(
        b"/pd/7461/raft",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap();
    store.mode.store(1, Ordering::SeqCst);
    let error = request(&url, "GET", "/pd/api/v1/cluster/status", "");
    assert_eq!(error.0, 500);
    assert!(
        error
            .1
            .as_str()
            .unwrap()
            .contains("revision is newer than the store"),
        "{error:?}"
    );
    store.mode.store(2, Ordering::SeqCst);
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-w",
                "\n%{http_code}",
                &format!("{url}/pd/api/v1/cluster/status"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let response = pending.await.unwrap();
    assert!(response.status.success());
    assert_eq!(
        String::from_utf8(response.stdout).unwrap(),
        "[PD:server:ErrServerNotStarted]server not started\n\n500"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.entries, inner.read(read).await.unwrap().entries);
}
