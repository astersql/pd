// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "grpc_resolved_ts_scenarios_test.rs"]
mod reports;
fn wire(url: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/min-resolved-ts{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (
        status.parse().unwrap(),
        serde_json::from_str(body).unwrap_or(json!(body)),
    )
}
async fn node(
    store: Arc<dyn astersql_pd_storage::MetaStore>,
    id: u64,
    member: u64,
) -> (astersql_pd_runtime::PdNode, String) {
    node_at(store, id, member, None).await
}
async fn node_at(
    store: Arc<dyn astersql_pd_storage::MetaStore>,
    id: u64,
    member: u64,
    url: Option<&str>,
) -> (astersql_pd_runtime::PdNode, String) {
    let listener = tokio::net::TcpListener::bind(
        url.map(|u| u.trim_start_matches("http://"))
            .unwrap_or("127.0.0.1:0"),
    )
    .await
    .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: id,
                member_id: member,
                name: format!("resolved-{member}"),
                client_url: url.clone(),
                peer_url: format!("http://peer{member}"),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_077_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 777, 1).await;
    assert_eq!(wire(&url, "/invalid").0, 500);
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 777).await;
    assert_eq!(wire(&url, "/invalid").0, 400);
    observe(&url, 777).await;
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(777);
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 100);
    let (mut follower, fu) = node(store, 777, 2).await;
    assert_eq!(wire(&fu, "/1"), wire(&url, "/1"));
    follower.shutdown().await.unwrap();
    n.shutdown().await.unwrap();
}
async fn observe(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let mut out = vec![];
    for ts in [42, 7, 0, u64::MAX, 100] {
        reports::report(&mut client, id, 1, ts).await.unwrap();
        let r = wire(url, "/1");
        assert_eq!(r.0, 200);
        assert_eq!(r.1["min_resolved_ts"], json!(ts));
        assert!(r.1["stores_min_resolved_ts"].is_null());
        out.push(json!(r));
    }
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while wire(url, "").1["min_resolved_ts"] != json!(100) {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for (store_id, state, node_state) in [(8, 2, 3), (9, 1, 2)] {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: reports::header(id),
                store: Some(astersql_pd_proto::metapb::Store {
                    id: store_id,
                    address: format!("store{store_id}"),
                    version: "8.5.0".into(),
                    state,
                    node_state,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        assert_eq!(
            wire(url, &format!("/{store_id}")).1["min_resolved_ts"],
            json!(u64::MAX)
        );
    }
    for suffix in [
        "/999",
        "/0",
        "/18446744073709551615",
        "/invalid",
        "/-1",
        "/+1",
        "/18446744073709551616",
        "?scope=1,999,1",
        "?scope=999",
        "?scope=cluster",
        "?scope=",
        "?scope=abc",
        "?scope=1,",
        "?scope=+1",
        "?scope=%2B1",
        "?scope=18446744073709551616",
        "?scope=1&scope=bad",
    ] {
        out.push(json!(wire(url, suffix)));
    }
    let r = wire(url, "?scope=1,999,1");
    assert_eq!(r.1["min_resolved_ts"], 100);
    assert_eq!(
        r.1["stores_min_resolved_ts"],
        json!({"1":100,"999":u64::MAX})
    );
    assert_eq!(wire(url, "/999").1["min_resolved_ts"], json!(u64::MAX));
    assert_eq!(wire(url, "/invalid").0, 400);
    let result = std::process::Command::new("curl")
        .args([
            "-sS",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data",
            r#"{"min-resolved-ts-persistence-interval":"0s"}"#,
            &format!("{url}/pd/api/v1/config"),
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let zero = wire(url, "/1");
    assert_eq!(zero.1["persist_interval"], "0s");
    assert!(zero.1.get("is_real_time").is_none());
    out.push(json!(zero));
    reports::report(&mut client, id, 1, 7).await.unwrap();
    assert_eq!(wire(url, "").1["min_resolved_ts"], 100);
    assert_eq!(wire(url, "/1").1["min_resolved_ts"], 7);
    out.push(json!(wire(url, "?scope=1,999")));
    json!(out)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_077_differential() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store, 778, 1).await;
    let mut rust = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut rust, 778).await;
    let expected = observe(&url, 778).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    let actual = observe(&go.url, id).await;
    println!("Go: {actual} Rust: {expected}");
    assert_eq!(actual, expected);
    go.stop().unwrap();
    n.shutdown().await.unwrap();
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct ResolvedStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for ResolvedStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, mut request: Read) -> astersql_pd_storage::Result<Snapshot> {
        if request.start.as_deref() == Some(b"/pd/779/raft") {
            match self.mode.load(std::sync::atomic::Ordering::SeqCst) {
                3 => request.revision = Some(i64::MAX),
                4 => {
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
async fn task_077_uninitialized_fault_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let f = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(f.store().await);
    let store = Arc::new(ResolvedStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let (mut n, url) = node(store.clone(), 779, 1).await;
    let mut c = PdClient::connect(url.clone()).await.unwrap();
    let boot = c
        .bootstrap(pdpb::BootstrapRequest {
            header: reports::header(779),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(astersql_pd_proto::metapb::Region {
                id: 2,
                peers: vec![astersql_pd_proto::metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    reports::report(&mut c, 779, 1, 42).await.unwrap();
    assert_eq!(wire(&url, "").1["min_resolved_ts"], json!(u64::MAX));
    assert_eq!(wire(&url, "/1").1["min_resolved_ts"], json!(u64::MAX));
    store.mode.store(3, Ordering::SeqCst);
    let failure = wire(&url, "");
    assert_eq!(failure.0, 500);
    assert!(
        failure.1.as_str().unwrap().contains("revision"),
        "{failure:?}"
    );
    store.mode.store(4, Ordering::SeqCst);
    let u = url.clone();
    let request = tokio::task::spawn_blocking(move || wire(&u, ""));
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    n.shutdown().await.unwrap();
    assert_eq!(
        request.await.unwrap(),
        (
            500,
            json!("[PD:server:ErrServerNotStarted]server not started\n")
        )
    );
    assert_eq!(
        astersql_pd_storage_endpoint::StorageEndpoint::new(inner)
            .load_min_resolved_ts(&astersql_pd_keypath::ClusterPaths::new(779))
            .await
            .unwrap(),
        0
    );
}
