// Copyright 2026 AsterSQL.
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
struct ConfigStore {
    inner: Arc<dyn MetaStore>,
    mode: std::sync::atomic::AtomicU8,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ConfigStore {
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
            .is_some_and(|key| key.ends_with(b"/config"))
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
async fn task_047_follower_recovery_fault_and_cancel() {
    use crate::http_member_test::request;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use serde_json::json;
    use std::{
        sync::atomic::{AtomicU8, Ordering},
        time::Duration,
    };
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(ConfigStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let paths = astersql_pd_keypath::ClusterPaths::new(7472);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(inner.clone());
    endpoint.save_config(&paths, &json!({"schedule":{"max-merge-region-size":37,"max-merge-region-keys":12345,"disable-remove-down-replica":"true"},"replication":{"max-replicas":5,"enable-placement-rules":"false"},"pd-server":{"flow-round-by-digit":4},"label-property":{"reject-leader":[{"key":"zone","value":"a"}]},"cluster-version":"8.5.0"})).await.unwrap();
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    let follower_cancel = CancellationToken::new();
    for id in 1..=2 {
        let listener =
            tokio::net::TcpListener::bind(if id == 1 { "[::1]:0" } else { "127.0.0.1:0" })
                .await
                .unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let cancel = if id == 2 {
            follower_cancel.clone()
        } else {
            CancellationToken::new()
        };
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel)
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7472,
                    member_id: id,
                    name: format!("config-{id}"),
                    client_url: url.clone(),
                    peer_url: format!("http://peer-{id}"),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    assert!(nodes[1].try_cluster().is_none());
    let mut progress = nodes[0].cluster().watch_schedule();
    tokio::time::timeout(Duration::from_secs(10), async {
        while progress.borrow().phase != astersql_pd_runtime::cluster_host::SchedulePhase::Running {
            progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let read = Read::range(
        paths.config_path().as_bytes(),
        b"",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap();
    let leader = request(&urls[0], "GET", "/pd/api/v1/config", "").1;
    let follower = request(&urls[1], "GET", "/pd/api/v1/config", "");
    assert_eq!(follower.0, 200, "{follower:?}");
    assert_eq!(follower.1["name"], "config-2");
    assert_eq!(follower.1["client-urls"], urls[1]);
    for (path, section) in [
        ("schedule", "schedule"),
        ("replicate", "replication"),
        ("pd-server", "pd-server"),
        ("label-property", "label-property"),
        ("cluster-version", "cluster-version"),
        ("replication-mode", "replication-mode"),
    ] {
        assert_eq!(follower.1[section], leader[section]);
        assert_eq!(
            request(&urls[1], "GET", &format!("/pd/api/v1/config/{path}"), ""),
            (200, leader[section].clone())
        );
    }
    assert_eq!(leader["schedule"]["max-merge-region-keys"], 12345);
    assert_eq!(leader["schedule"]["enable-remove-down-replica"], "false");
    assert!(
        leader["schedule"]
            .get("disable-remove-down-replica")
            .is_none()
    );
    assert_eq!(leader["replication"]["max-replicas"], 5);
    assert_eq!(leader["pd-server"]["flow-round-by-digit"], 4);
    assert_eq!(leader["cluster-version"], "8.5.0");
    assert_eq!(
        leader["label-property"],
        json!({"reject-leader":[{"key":"zone","value":"a"}]})
    );
    assert_eq!(
        request(&urls[1], "GET", "/pd/api/v1/config/default", ""),
        request(&urls[0], "GET", "/pd/api/v1/config/default", "")
    );
    store.mode.store(1, Ordering::SeqCst);
    let error = request(&urls[1], "GET", "/pd/api/v1/config", "");
    assert_eq!(error.0, 500);
    assert!(
        error
            .1
            .as_str()
            .unwrap()
            .contains("revision is newer than the store"),
        "{error:?}"
    );
    assert_eq!(request(&urls[1], "GET", "/pd/api/v1/config", ""), follower);
    store.mode.store(2, Ordering::SeqCst);
    let url = urls[1].clone();
    let pending =
        tokio::task::spawn_blocking(move || request(&url, "GET", "/pd/api/v1/config", ""));
    tokio::time::timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    follower_cancel.cancel();
    assert_eq!(
        pending.await.unwrap(),
        (
            500,
            json!("[PD:server:ErrServerNotStarted]server not started")
        )
    );
    tokio::time::timeout(Duration::from_secs(5), nodes[1].shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.entries, inner.read(read).await.unwrap().entries);
    nodes[0].shutdown().await.unwrap();
}
