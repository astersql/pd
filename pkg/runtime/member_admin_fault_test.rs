// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(super) struct MemberStore {
    pub(super) inner: Arc<dyn MetaStore>,
    pub(super) mode: std::sync::atomic::AtomicU8,
    pub(super) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for MemberStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        // Leadership administration gates at its fenced validation transaction.
        if writes.is_empty() && compares.iter().any(|compare| matches!(compare,
            Compare::CreateRevision(key, 0) if String::from_utf8_lossy(key).contains("/rust/removed-members/"))) {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                4 => { self.entered.notify_one(); std::future::pending::<()>().await; },
                5 => writes.push(Mutation::put(b"/task045/oversized", &vec![0; 2 * 1024 * 1024])),
                other => { self.mode.store(other, std::sync::atomic::Ordering::SeqCst); }
            }
        }
        if self.mode.load(std::sync::atomic::Ordering::SeqCst) == 3
            && writes.iter().any(|write| matches!(write, Mutation::Delete { key } if key.ends_with(b"/leader_priority")))
        {
            self.mode.store(0, std::sync::atomic::Ordering::SeqCst);
            writes.push(Mutation::put(b"/task044/oversized", &vec![0; 2 * 1024 * 1024]));
        }
        if writes.iter().any(
            |write| matches!(write, Mutation::Put {key, ..} if key.ends_with(b"/leader_priority")),
        ) {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::put(
                    b"/task044/oversized",
                    &vec![0; 2 * 1024 * 1024],
                )),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_044_priority_transaction_fault_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::node_test::fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let store = Arc::new(MemberStore {
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
                cluster_id: 7441,
                member_id: 1,
                name: "member-1".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let path = "/pd/api/v1/members/name/member-1";
    use crate::member_admin_test::request;
    assert_eq!(
        request(&url, "POST", path, r#"{"leader-priority":5}"#).0,
        200
    );
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        request(&url, "POST", path, r#"{"leader-priority":6}"#).0,
        500
    );
    let read = Read::range(
        b"/pd/7441/member/1/leader_priority",
        b"",
        astersql_pd_storage::Consistency::Linearizable,
    );
    assert_eq!(
        inner.read(read.clone()).await.unwrap().entries[0].value,
        b"5"
    );
    store.mode.store(2, Ordering::SeqCst);
    let pending_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        request(&pending_url, "POST", path, r#"{"leader-priority":7}"#)
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(pending.await.unwrap().0, 500);
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries[0].value, b"5");
}
