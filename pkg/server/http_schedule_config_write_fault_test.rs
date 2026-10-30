// Copyright 2026 AsterSQL.
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct ConfigStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ConfigStore {
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
        for write in &mut writes {
            if let Mutation::Put { key, lease, .. } = write {
                if key.ends_with(b"/config") {
                    match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                        1 => *lease = Some(i64::MAX),
                        2 => {
                            self.entered.notify_one();
                            std::future::pending::<()>().await;
                        }
                        _ => {}
                    }
                }
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
async fn task_048_fault_cancel_and_concurrent() {
    use crate::http_member_test::request;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(ConfigStore {
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
                cluster_id: 7482,
                member_id: 1,
                name: "fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut progress = node.cluster().watch_schedule();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while progress.borrow().phase != astersql_pd_runtime::cluster_host::SchedulePhase::Running {
            progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7482,
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(
        request(
            &follower_url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"max-snapshot-count":88}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/schedule", "").1["max-snapshot-count"],
        88
    );
    follower.shutdown().await.unwrap();
    let read = Read::range(
        b"/pd/7482/config",
        b"",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap().entries;
    let config = request(&url, "GET", "/pd/api/v1/config/schedule", "");
    store.mode.store(1, Ordering::SeqCst);
    let failure = request(
        &url,
        "POST",
        "/pd/api/v1/config/schedule",
        r#"{"max-snapshot-count":999}"#,
    );
    assert_eq!(failure.0, 500, "{failure:?}");
    assert!(failure.1.as_str().unwrap().contains("lease"), "{failure:?}");
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/schedule", ""),
        config
    );
    let mut tasks = Vec::new();
    for body in [
        r#"{"max-snapshot-count":123}"#,
        r#"{"max-pending-peer-count":456}"#,
    ] {
        let url = url.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            request(&url, "POST", "/pd/api/v1/config/schedule", body)
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().0, 200);
    }
    let config = request(&url, "GET", "/pd/api/v1/config/schedule", "").1;
    assert_eq!(config["max-snapshot-count"], 123);
    assert_eq!(config["max-pending-peer-count"], 456);
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
    let write_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                "POST",
                "--data-binary",
                r#"{"max-snapshot-count":789}"#,
                "-w",
                "\n%{http_code}",
                &format!("{write_url}/pd/api/v1/config/schedule"),
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let response = pending.await.unwrap();
    let (body, status) = response.rsplit_once('\n').unwrap();
    assert_eq!(status, "500");
    assert!(
        body.contains("[PD:server:ErrServerNotStarted]server not started"),
        "{body}"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}
