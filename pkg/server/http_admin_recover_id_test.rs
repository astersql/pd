// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
fn wire(url: &str, path: &str, body: &str) -> (u16, String, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            "POST",
            "-H",
            "Content-Type: text/plain",
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/admin/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into(), content.to_lowercase())
}
fn reset(url: &str, body: &str) -> (u16, String, String) {
    wire(url, "base-alloc-id", body)
}
async fn alloc(url: &str, cluster_id: u64) -> u64 {
    let r = PdClient::connect(url.to_owned())
        .await
        .unwrap()
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id,
                ..Default::default()
            }),
            count: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    r.id
}
async fn boundary(store: &dyn MetaStore, id: u64) -> u64 {
    let s = store
        .read(Read::range(
            format!("/pd/{id}/alloc_id").as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    u64::from_be_bytes(s.entries[0].value.as_slice().try_into().unwrap())
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
                name: format!("marker-{member}"),
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

async fn observe(url: &str, id: u64) -> Vec<(u16, String, String)> {
    let mut result = Vec::new();
    for body in [
        "{}",
        "null",
        r#"{"id":1}"#,
        r#"{"id":""}"#,
        r#"{"id":"+1"}"#,
        r#"{"id":"18446744073709551616"}"#,
    ] {
        let r = reset(url, body);
        assert_eq!(r.0, 400, "{r:?}");
        result.push(r);
    }
    let r = reset(url, r#"{"id":"10000"}"#);
    assert_eq!(r.0, 403, "{r:?}");
    result.push(r);
    assert_eq!(wire(url, "cluster/markers/snapshot-recovering", "").0, 200);
    for (body, status) in [
        (r#"{"id":"10000"}"#, 200),
        (r#"{"id":"18446744073709551615"}"#, 500),
    ] {
        let r = reset(url, body);
        assert_eq!(r.0, status, "{r:?}");
        result.push(r);
    }
    assert_eq!(alloc(url, id).await, 10001);
    // Explicit administrative reset may lower or equal the base, as Go SetBase does.
    assert_eq!(reset(url, r#"{"id":"0"}"#).0, 200);
    assert_eq!(alloc(url, id).await, 1);
    assert_eq!(reset(url, r#"{"id":"10000"}"#).0, 200);
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_075_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 775, 1).await;
    observe(&url, 775).await;
    assert_eq!(boundary(store.as_ref(), 775).await, 11000);
    let mut joins = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let url = url.clone();
        joins.spawn(async move { alloc(&url, 775).await });
    }
    let mut ids = BTreeSet::new();
    while let Some(id) = joins.join_next().await {
        assert!(ids.insert(id.unwrap()));
    }
    assert_eq!(ids.len(), 32);
    assert_eq!(*ids.first().unwrap(), 10001);
    let (mut follower, follower_url) = node(store.clone(), 775, 2).await;
    assert_eq!(reset(&follower_url, r#"{"id":"20000"}"#).0, 200);
    assert_eq!(alloc(&url, 775).await, 20001);
    follower.shutdown().await.unwrap();
    let saved = boundary(store.as_ref(), 775).await;
    n.shutdown().await.unwrap();
    let (mut n, url) = node_at(store.clone(), 775, 1, Some(&url)).await;
    assert!(alloc(&url, 775).await > saved);
    n.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_075_differential() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 7751, 1).await;
    let rust = observe(&url, 7751).await;
    let (mut follower, fu) = node(store.clone(), 7751, 2).await;
    assert_eq!(reset(&fu, r#"{"id":"20000"}"#).0, 200);
    assert_eq!(alloc(&url, 7751).await, 20001);
    follower.shutdown().await.unwrap();
    n.shutdown().await.unwrap();
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
    let oracle = observe(&go.url, id).await;
    // Both implementations preserve the durable boundary across clean restart.
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    assert_eq!(reset(&go.url, r#"{"id":"40000"}"#).0, 200);
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    assert!(alloc(&go.url, id).await > 41000);
    let join = format!("--join={}", go.url);
    let mut follower = astersql_pd_compat_tests::harness::Server::start_with_args(
        &binary,
        &["--name=recover-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    assert_eq!(reset(&follower.url, r#"{"id":"20000"}"#).0, 200);
    assert_eq!(alloc(&go.url, id).await, 20001);
    follower.stop().unwrap();
    let (mut n, url) = node_at(store, 7751, 1, Some(&url)).await;
    assert!(alloc(&url, 7751).await > 21000);
    n.shutdown().await.unwrap();
    go.stop().unwrap();
    println!("Go: {oracle:?} Rust: {rust:?}");
    assert_eq!(oracle, rust);
}
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Revision,
    Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct RecoveryStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for RecoveryStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        let mut request = request;
        if request
            .start
            .as_ref()
            .is_some_and(|key| key.windows(9).any(|w| w == b"/markers/"))
        {
            match self.mode.load(std::sync::atomic::Ordering::SeqCst) {
                3 => {
                    request.revision = Some(i64::MAX);
                }
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
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        let target = writes.iter().any(|write| matches!(write, Mutation::Delete {key} | Mutation::Put {key, ..} if key.ends_with(b"/alloc_id")));
        if target {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::Put {
                    key: b"/task075/failure".to_vec(),
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
async fn task_075_fault_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let f = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(f.store().await);
    let store = Arc::new(RecoveryStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let (mut n, url) = node(store.clone(), 7752, 1).await;
    assert_eq!(wire(&url, "cluster/markers/snapshot-recovering", "").0, 200);
    assert_eq!(reset(&url, r#"{"id":"10000"}"#).0, 200);
    let saved = boundary(inner.as_ref(), 7752).await;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(reset(&url, r#"{"id":"20000"}"#).0, 500);
    assert_eq!(boundary(inner.as_ref(), 7752).await, saved);
    // A rejected transaction does not claim a new durable reservation.
    assert_eq!(reset(&url, r#"{"id":"30000"}"#).0, 200);
    assert_eq!(alloc(&url, 7752).await, 30001);
    store.mode.store(3, Ordering::SeqCst);
    assert_eq!(reset(&url, r#"{"id":"40000"}"#).0, 500);
    assert_eq!(boundary(inner.as_ref(), 7752).await, 31000);
    store.mode.store(2, Ordering::SeqCst);
    let u = url.clone();
    let request = tokio::task::spawn_blocking(move || reset(&u, r#"{"id":"50000"}"#));
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    n.shutdown().await.unwrap();
    assert_eq!(request.await.unwrap().0, 500);
    assert_eq!(boundary(inner.as_ref(), 7752).await, 31000);
    // Cancellation before commit leaves the durable boundary usable after restart.
    let (mut n, url) = node_at(store, 7752, 1, Some(&url)).await;
    assert!(alloc(&url, 7752).await > 31000);
    n.shutdown().await.unwrap();
}
