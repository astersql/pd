// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn wire(url: &str, method: &str, kind: &str) -> (u16, String, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            method,
            "-H",
            "pd-allow-follower-handle: true",
            "-H",
            "Content-Type: text/plain",
            "--data-binary",
            "ignored body",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/admin/cluster/markers/{kind}?ignored=true"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into(), content.to_lowercase())
}
async fn rpc(url: &str, expected: bool) {
    let r = PdClient::connect(url.to_owned())
        .await
        .unwrap()
        .is_snapshot_recovering(pdpb::IsSnapshotRecoveringRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: u64::MAX,
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    assert_eq!(r.marked, expected);
}
async fn observe(url: &str) -> Value {
    let mut out = vec![];
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        for (method, expected) in [
            ("GET", false),
            ("DELETE", false),
            ("POST", true),
            ("POST", true),
            ("GET", true),
        ] {
            let r = wire(url, method, kind);
            assert_eq!(r.0, 200, "{r:?}");
            if method == "GET" {
                assert_eq!(
                    serde_json::from_str::<Value>(&r.1).unwrap(),
                    json!({"marked":expected})
                );
            } else {
                assert_eq!(r.1, "");
            }
            out.push(json!(r));
            rpc(
                url,
                if kind == "snapshot-recovering" {
                    expected
                } else {
                    true
                },
            )
            .await;
        }
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_074_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 774, 1).await;
    // Marker APIs are available before bootstrap.
    observe(&url).await;
    let (mut follower, fu) = node(store.clone(), 774, 2).await;
    rpc(&fu, true).await;
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        let key = format!("/pd/774/cluster/markers/{kind}");
        let read = Read::range(
            key.as_bytes(),
            b"",
            astersql_pd_storage::Consistency::Linearizable,
        );
        let before = store.read(read.clone()).await.unwrap().entries;
        assert_eq!(before[0].value, b"on");
        assert_eq!(wire(&fu, "POST", kind).0, 200);
        assert_eq!(store.read(read.clone()).await.unwrap().entries, before);
        // Presence, including an empty value, is the marker. Prefix neighbors are not.
        store
            .transact(vec![], vec![Mutation::put(key.as_bytes(), b"")])
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&wire(&url, "GET", kind).1).unwrap(),
            json!({"marked":true})
        );
        assert_eq!(wire(&url, "POST", kind).0, 200);
        assert!(
            store.read(read.clone()).await.unwrap().entries[0]
                .value
                .is_empty()
        );
    }
    follower.shutdown().await.unwrap();
    n.shutdown().await.unwrap();
    let (mut n, url) = node_at(store.clone(), 774, 1, Some(&url)).await;
    rpc(&url, true).await;
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        assert!(
            serde_json::from_str::<Value>(&wire(&url, "GET", kind).1).unwrap()["marked"]
                .as_bool()
                .unwrap()
        );
        for _ in 0..2 {
            assert_eq!(wire(&url, "DELETE", kind).0, 200);
        }
        let key = format!("/pd/774/cluster/markers/{kind}/neighbor");
        store
            .transact(vec![], vec![Mutation::put(key.as_bytes(), b"on")])
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&wire(&url, "GET", kind).1).unwrap(),
            json!({"marked":false})
        );
    }
    rpc(&url, false).await;
    n.shutdown().await.unwrap();
    let (mut n, url) = node_at(store, 774, 1, Some(&url)).await;
    rpc(&url, false).await;
    assert_eq!(
        serde_json::from_str::<Value>(&wire(&url, "GET", "pitr-restore-mode").1).unwrap(),
        json!({"marked": false})
    );
    n.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_074_differential() {
    let f = crate::etcd_fixture::Fixture::start();
    let (mut n, url) = node(Arc::new(f.store().await), 7741, 1).await;
    let rust = observe(&url).await;
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7741).await;
    n.shutdown().await.unwrap();
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let result = observe(&go.url).await;
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
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    rpc(&go.url, true).await;
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        assert_eq!(
            serde_json::from_str::<Value>(&wire(&go.url, "GET", kind).1).unwrap(),
            json!({"marked":true})
        );
        for _ in 0..2 {
            assert_eq!(wire(&go.url, "DELETE", kind).0, 200);
        }
    }
    rpc(&go.url, false).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    rpc(&go.url, false).await;
    assert_eq!(
        serde_json::from_str::<Value>(&wire(&go.url, "GET", "pitr-restore-mode").1).unwrap(),
        json!({"marked":false})
    );
    let join = format!("--join={}", go.url);
    let mut follower = astersql_pd_compat_tests::harness::Server::start_with_args(
        &binary,
        &["--name=marker-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    rpc(&follower.url, false).await;
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        assert_eq!(wire(&follower.url, "POST", kind).0, 200);
        assert_eq!(
            serde_json::from_str::<Value>(&wire(&follower.url, "GET", kind).1).unwrap(),
            json!({"marked": true})
        );
    }
    follower.stop().unwrap();
    go.stop().unwrap();
    println!("Go: {result} Rust: {rust}");
    assert_eq!(result, rust);
}
use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct MarkerStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for MarkerStore {
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
        let target = writes.iter().any(|write| matches!(write, Mutation::Delete {key} | Mutation::Put {key, ..} if key.windows(9).any(|w|w==b"/markers/")));
        if target {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::Put {
                    key: b"/task074/failure".to_vec(),
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
async fn task_074_fault_and_cancel() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let f = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(f.store().await);
    let store = Arc::new(MarkerStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let (mut n, url) = node(store.clone(), 7742, 1).await;
    for kind in ["snapshot-recovering", "pitr-restore-mode"] {
        for method in ["POST", "DELETE"] {
            if method == "DELETE" {
                assert_eq!(wire(&url, "POST", kind).0, 200);
            }
            let key = format!("/pd/7742/cluster/markers/{kind}");
            let read = Read::range(
                key.as_bytes(),
                b"",
                astersql_pd_storage::Consistency::Linearizable,
            );
            let before = inner.read(read.clone()).await.unwrap().entries;
            store.mode.store(1, Ordering::SeqCst);
            assert_eq!(wire(&url, method, kind).0, 500);
            assert_eq!(inner.read(read).await.unwrap().entries, before);
        }
        store.mode.store(3, Ordering::SeqCst);
        assert_eq!(wire(&url, "GET", kind).0, 500);
        let r = PdClient::connect(url.clone())
            .await
            .unwrap()
            .is_snapshot_recovering(pdpb::IsSnapshotRecoveringRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert!(!r.marked);
        assert_eq!(
            r.header.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::Unknown as i32
        );
        store.mode.store(0, Ordering::SeqCst);
    }
    store.mode.store(2, Ordering::SeqCst);
    let u = url.clone();
    let request = tokio::task::spawn_blocking(move || wire(&u, "DELETE", "snapshot-recovering"));
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    n.shutdown().await.unwrap();
    assert_eq!(request.await.unwrap().0, 500);
    let key = b"/pd/7742/cluster/markers/snapshot-recovering";
    assert_eq!(
        inner
            .read(Read::range(
                key,
                b"",
                astersql_pd_storage::Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"on"
    );
    let (mut n, url) = node_at(store.clone(), 7742, 1, Some(&url)).await;
    store.mode.store(4, Ordering::SeqCst);
    let request = tokio::spawn(async move {
        PdClient::connect(url)
            .await
            .unwrap()
            .is_snapshot_recovering(pdpb::IsSnapshotRecoveringRequest::default())
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    n.shutdown().await.unwrap();
    let response = request.await.unwrap().unwrap().into_inner();
    assert_eq!(
        response.header.unwrap().error.unwrap().message,
        "context canceled"
    );
}
