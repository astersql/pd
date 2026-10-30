// Copyright 2026 AsterSQL.
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct RuleStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for RuleStore {
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
                if key.windows(7).any(|w| w == b"/rules/") {
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
async fn task_068_storage_failure_and_cancel() {
    use crate::http_member_test::request;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(RuleStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7684,
                member_id: 1,
                name: "rule-fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 7684).await;
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    let body = r#"{"group_id":"fault","id":"one","role":"voter","count":2}"#;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(request(&url, "POST", "/pd/api/v1/config/rule", body).0, 500);
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/rule/fault/one", "").0,
        404
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(inner.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7684);
    assert!(
        endpoint
            .load_rule(&paths, "6661756c74-6f6e65")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(request(&url, "POST", "/pd/api/v1/config/rule", body).0, 200);
    let before = request(&url, "GET", "/pd/api/v1/config/rule/fault/one", "");
    let persisted = endpoint
        .load_rule(&paths, "6661756c74-6f6e65")
        .await
        .unwrap();
    store.mode.store(2, Ordering::SeqCst);
    let target = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "10",
                "-X",
                "POST",
                "--data-binary",
                r#"{"group_id":"fault","id":"one","role":"voter","count":1}"#,
                "-w",
                "%{http_code}",
                &format!("{target}/pd/api/v1/config/rule"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let result = pending.await.unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout).unwrap().ends_with("500"));
    node.shutdown().await.unwrap();
    assert_eq!(
        endpoint
            .load_rule(&paths, "6661756c74-6f6e65")
            .await
            .unwrap(),
        persisted
    );
    assert_eq!(before.1["count"], 2);
}
