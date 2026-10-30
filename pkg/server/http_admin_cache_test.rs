// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_073_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let (mut n, url) = node(Arc::new(fixture.store().await), 772, 1).await;
    let response = wire(&url, "DELETE", "/admin/cache/region/10", false);
    n.shutdown().await.unwrap();
    assert_eq!(
        response.0, 500,
        "bootstrap must precede body validation: {response:?}"
    );
}
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
use std::time::Duration;
fn wire(url: &str, method: &str, path: &str, follower: bool) -> (u16, Value) {
    let mut c = std::process::Command::new("curl");
    c.args([
        "-sS",
        "--max-time",
        "10",
        "-X",
        method,
        "-w",
        "\n%{http_code}\n%{content_type}",
    ]);
    if follower {
        c.args(["-H", "pd-allow-follower-handle: true"]);
    }
    let o = c.arg(format!("{url}/pd/api/v1{path}")).output().unwrap();
    assert!(o.status.success(), "{o:?}");
    let text = String::from_utf8(o.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    if method == "DELETE" && content.starts_with("application/json") {
        assert_eq!(content.to_lowercase(), "application/json; charset=utf-8");
    }
    (
        code.parse().unwrap(),
        serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
    )
}
async fn observe(url: &str, id: u64) -> Value {
    let mut cold = Vec::new();
    for path in [
        "/admin/cache/region/bad",
        "/admin/storage/region/10",
        "/admin/cache/regions",
    ] {
        let r = wire(url, "DELETE", path, false);
        assert_eq!(r.0, 500);
        cold.push(json!([path, r]));
    }
    let mut c = PdClient::connect(url.to_owned()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut c, id).await;
    // The existing Go local backend batches heartbeat writes for three seconds.
    // Establish a durable baseline before deleting metadata, avoiding its known
    // late-batch resurrection when DeleteRegion races an unflushed heartbeat.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let mut out = cold;
    for (path, status) in [
        ("/admin/cache/region/abc", 400),
        ("/admin/cache/region/0", 200),
        ("/admin/cache/region/18446744073709551615", 200),
        ("/admin/storage/region/0", 400),
        ("/admin/cache/region/-1", 400),
        ("/admin/cache/region/18446744073709551616", 400),
        ("/admin/storage/region/abc", 400),
        ("/admin/storage/region/999", 400),
        ("/admin/cache/region/999", 200),
        ("/admin/storage/region/10", 200),
        ("/admin/storage/region/10", 400),
        ("/admin/cache/region/11", 200),
        ("/admin/storage/region/11", 400),
        ("/admin/cache/regions", 200),
    ] {
        let response = wire(url, "DELETE", path, false);
        assert_eq!(response.0, status, "{path}: {response:?}");
        out.push(json!([path, response]));
    }
    for region_id in [10, 11, 12, 999] {
        let response = c
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: crate::grpc_region_lookup_scenarios_test::header(id),
                region_id,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.region.is_none());
        assert!(response.header.unwrap().error.is_none());
        let response = wire(url, "GET", &format!("/region/id/{region_id}"), false);
        assert_eq!(response.0, 404);
        out.push(json!(response));
    }
    json!(out)
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
                name: format!("cache-{member}"),
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
async fn rust_observe() -> Value {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 773, 1).await;
    let result = observe(&url, 773).await;
    let endpoint = StorageEndpoint::new(store.clone());
    for id in [10, 11, 12] {
        assert_eq!(
            endpoint
                .load_region(&ClusterPaths::new(773), id)
                .await
                .unwrap()
                .is_some(),
            id != 10,
            "region {id} persistence"
        );
    }
    n.shutdown().await.unwrap();
    let (mut n, url) = node_at(store, 773, 1, Some(&url)).await;
    for id in [10, 11, 12] {
        assert_eq!(
            wire(&url, "GET", &format!("/region/id/{id}"), false).0,
            if id == 10 { 404 } else { 200 }
        );
    }
    n.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_073_deletion_and_restart() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_073_differential() {
    let rust = rust_observe().await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let id = PdClient::connect(go.url.clone())
        .await
        .unwrap()
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let result = observe(&go.url, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    for id in [10, 11, 12] {
        assert_eq!(
            wire(&go.url, "GET", &format!("/region/id/{id}"), false).0,
            if id == 10 { 404 } else { 200 }
        );
    }
    let (rust_follower, go_follower) = follower_differential(&binary, &go.url).await;
    assert_eq!(rust_follower, go_follower);
    go.stop().unwrap();
    println!(
        "Go: {result}\nRust: {rust}\nGo follower: {go_follower}\nRust follower: {rust_follower}"
    );
    assert_eq!(result, rust);
}
async fn synced(url: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if wire(url, "GET", "/region/id/10", true).0 == 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_073_follower_reset() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut leader, url) = node(store.clone(), 7731, 1).await;
    let mut c = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut c, 7731).await;
    let (mut follower, fu) = node(store, 7731, 2).await;
    synced(&fu).await;
    assert_eq!(wire(&fu, "DELETE", "/admin/storage/region/10", true).0, 500);
    for (path, message) in [
        (
            "/admin/cache/region/10",
            "The region is removed from follower cache and the follower starts to resync regions from leader.",
        ),
        (
            "/admin/cache/regions",
            "All regions are removed from follower cache and the follower starts to resync regions from leader.",
        ),
    ] {
        assert_eq!(wire(&fu, "DELETE", path, true), (200, json!(message)));
        synced(&fu).await;
        assert_eq!(wire(&url, "GET", "/region/id/10", false).0, 200);
    }
    follower.shutdown().await.unwrap();
    leader.shutdown().await.unwrap();
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct RegionStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for RegionStore {
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
        let target = writes.iter().any(|write| matches!(write, Mutation::Delete {key} if key.windows(8).any(|w|w==b"/raft/r/")));
        if target {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::Put {
                    key: b"/task073/failure".to_vec(),
                    value: vec![],
                    lease: Some(i64::MAX),
                }),
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
async fn task_073_fault_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(RegionStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let (mut n, url) = node(store.clone(), 7732, 1).await;
    let mut c = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut c, 7732).await;
    let read = Read::prefix(
        b"/pd/7732/raft/r/",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        wire(&url, "DELETE", "/admin/storage/region/10", false),
        (200, json!("failed to delete region from storage."))
    );
    assert_eq!(wire(&url, "GET", "/region/id/10", false).0, 200);
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    store.mode.store(2, Ordering::SeqCst);
    let u = url.clone();
    let request =
        tokio::task::spawn_blocking(move || wire(&u, "DELETE", "/admin/storage/region/10", false));
    tokio::time::timeout(Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    n.shutdown().await.unwrap();
    assert_eq!(request.await.unwrap().0, 500);
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}

async fn follower_observe(leader: &str, follower: &str) -> Value {
    synced(follower).await;
    let denied = wire(follower, "DELETE", "/admin/storage/region/10", true);
    assert_eq!(denied.0, 500);
    let invalid = wire(follower, "DELETE", "/admin/cache/region/bad", true);
    assert_eq!(invalid.0, 400);
    let mut out = vec![json!(denied), json!(invalid)];
    for path in [
        "/admin/cache/region/10",
        "/admin/cache/region/999",
        "/admin/cache/regions",
    ] {
        let response = wire(follower, "DELETE", path, true);
        assert_eq!(response.0, 200);
        out.push(json!(response));
        synced(follower).await;
        assert_eq!(wire(leader, "GET", "/region/id/10", false).0, 200);
    }
    // Without the explicit header the same route forwards to the leader.
    let forwarded = wire(follower, "DELETE", "/admin/cache/region/999", false);
    assert_eq!(
        forwarded,
        (200, json!("The region is removed from server cache."))
    );
    out.push(json!(forwarded));
    json!(out)
}
async fn follower_differential(binary: &std::path::Path, _go_leader: &str) -> (Value, Value) {
    let mut go = astersql_pd_compat_tests::harness::Server::start(binary).unwrap();
    go.ready().unwrap();
    let mut c = PdClient::connect(go.url.clone()).await.unwrap();
    let cluster_id = c
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    crate::grpc_region_lookup_scenarios_test::prepare(&mut c, cluster_id).await;
    let join = format!("--join={}", go.url);
    let mut follower = astersql_pd_compat_tests::harness::Server::start_with_args(
        binary,
        &["--name=cache-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    let mut go_result = follower_observe(&go.url, &follower.url).await;
    // Go reloads persisted cluster options at startup; a local file alone is
    // overridden by the leader's existing use-region-storage value.
    assert_eq!(
        crate::http_member_test::request(
            &go.url,
            "POST",
            "/pd/api/v1/config",
            r#"{"use-region-storage":"false"}"#
        )
        .0,
        200
    );
    let config = std::env::temp_dir().join(format!("task073-disabled-{}.toml", std::process::id()));
    std::fs::write(&config, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let config_arg = format!("--config={}", config.display());
    // etcd's strict membership admission requires a sustained healthy window
    // after adding the second member before another member can join.
    tokio::time::sleep(Duration::from_secs(10)).await;
    let mut disabled = astersql_pd_compat_tests::harness::Server::start_with_args(
        binary,
        &[
            "--name=cache-disabled",
            "--initial-cluster=",
            &join,
            &config_arg,
        ],
    )
    .unwrap();
    disabled.ready().unwrap();
    let denied = wire(&disabled.url, "DELETE", "/admin/cache/regions", true);
    assert_eq!(denied, (500, json!("region storage is disabled")));
    go_result.as_array_mut().unwrap().push(json!(denied));
    disabled.stop().unwrap();
    follower.stop().unwrap();
    std::fs::remove_file(config).unwrap();
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let (mut leader, url) = node(store.clone(), cluster_id, 1).await;
    let mut c = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut c, cluster_id).await;
    let (mut follower, fu) = node(store, cluster_id, 2).await;
    let mut rust = follower_observe(&url, &fu).await;
    assert_eq!(
        crate::http_member_test::request(
            &url,
            "POST",
            "/pd/api/v1/config",
            r#"{"use-region-storage":"false"}"#
        )
        .0,
        200
    );
    let denied = wire(&fu, "DELETE", "/admin/cache/regions", true);
    assert_eq!(denied, (500, json!("region storage is disabled")));
    rust.as_array_mut().unwrap().push(json!(denied));
    follower.shutdown().await.unwrap();
    leader.shutdown().await.unwrap();
    (rust, go_result)
}
