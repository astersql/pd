// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_052_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 752,
                member_id: 1,
                name: "state".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = request(&url, "DELETE", "/pd/api/v1/store/1", "");
    node.shutdown().await.unwrap();
    assert_eq!(
        response.0, 500,
        "unbootstrapped store mutation: {response:?}"
    );
}

async fn observe(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let mut output = vec![];
    let r = request(url, "DELETE", "/pd/api/v1/store/1", "");
    assert_eq!(r.0, 500);
    output.push(json!(r));
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1".into(),
                node_state: 1,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
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
    assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    for (method, path, code) in [
        ("DELETE", "/pd/api/v1/store/1", 400),
        ("DELETE", "/pd/api/v1/store/999", 404),
        ("DELETE", "/pd/api/v1/store/nope", 400),
        ("POST", "/pd/api/v1/store/1/state?state=Tombstone", 400),
        ("POST", "/pd/api/v1/store/1/state", 400),
        ("POST", "/pd/api/v1/store/1/state?state=uP", 200),
    ] {
        let r = request(url, method, path, "");
        assert_eq!(r.0, code, "{path}: {r:?}");
        output.push(json!([method, path, r]));
    }
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    for (n, state, destroyed, engine) in [
        (4, 0, false, ""),
        (5, 0, false, ""),
        (6, 0, false, ""),
        (7, 2, false, ""),
        (10, 2, false, ""),
        (8, 1, true, ""),
        (9, 0, false, "tiflash"),
    ] {
        let r = client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: n,
                    address: format!("store-{n}"),
                    state,
                    node_state: match state {
                        1 => 2,
                        2 => 3,
                        _ => 1,
                    },
                    physically_destroyed: destroyed,
                    last_heartbeat: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as i64,
                    labels: if engine.is_empty() {
                        vec![]
                    } else {
                        vec![metapb::StoreLabel {
                            key: "engine".into(),
                            value: engine.into(),
                        }]
                    },
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    }
    region_with_peer(url, id, Some(7), 2).await;
    let partial = request(url, "DELETE", "/pd/api/v1/stores/remove-tombstone", "");
    assert_eq!(partial, (500, json!("failed stores: 7")));
    assert_eq!(request(url, "GET", "/pd/api/v1/store/10", "").0, 404);
    output.push(json!(["partial-cleanup", partial]));
    region_with_peer(url, id, None, 3).await;
    for (method, path, code) in [
        ("DELETE", "/pd/api/v1/store/4", 200),
        ("DELETE", "/pd/api/v1/store/4", 200),
        ("POST", "/pd/api/v1/store/4/state?state=UP", 200),
        ("POST", "/pd/api/v1/store/4/state?state=Offline", 200),
        ("POST", "/pd/api/v1/store/7/state?state=Up", 410),
        ("DELETE", "/pd/api/v1/store/7", 410),
        ("POST", "/pd/api/v1/store/8/state?state=Up", 400),
        ("DELETE", "/pd/api/v1/store/8", 400),
        ("DELETE", "/pd/api/v1/store/8?force=false", 200),
        ("DELETE", "/pd/api/v1/store/5?force=false", 200),
        ("POST", "/pd/api/v1/store/5/state?state=Up", 400),
        ("DELETE", "/pd/api/v1/store/9", 200),
        ("DELETE", "/pd/api/v1/stores/remove-tombstone", 200),
        ("DELETE", "/pd/api/v1/stores/remove-tombstone", 200),
    ] {
        let r = request(url, method, path, "");
        assert_eq!(r.0, code, "{path}: {r:?}");
        output.push(json!([method, path, r]));
        if path == "/pd/api/v1/store/4/state?state=UP" {
            let limits =
                request(url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"]["4"].clone();
            assert_eq!(limits["remove-peer"].as_f64(), Some(15.0), "{limits}");
            output.push(json!(["restored", limits["remove-peer"].as_f64()]));
        }
    }
    for n in [1, 4, 5, 6, 7, 8, 9] {
        let r = client
            .get_store(pdpb::GetStoreRequest {
                header: header.clone(),
                store_id: n,
            })
            .await
            .unwrap()
            .into_inner();
        let meta=r.store.map(|s|json!({"id":s.id,"state":s.state,"node_state":s.node_state,"destroyed":s.physically_destroyed}));
        output.push(json!([
            n,
            meta,
            r.header.unwrap().error.map(|e| (e.r#type, e.message))
        ]));
    }
    let limits = request(url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"].clone();
    for id in ["4", "5", "9"] {
        assert_eq!(
            limits[id]["remove-peer"].as_f64(),
            Some(100000000.0),
            "{limits}"
        );
    }
    output.push(json!([
        "limits",
        (["4", "5", "9"].map(|id| json!(limits[id]["remove-peer"].as_f64().unwrap())))
    ]));
    json!(output)
}
async fn rust_observe() -> Value {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = restart_listener("127.0.0.1:0".parse().unwrap());
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let config = NodeConfig {
        cluster_id: 752,
        member_id: 1,
        name: "state".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut result = observe(&url, 752).await;
    node.shutdown().await.unwrap();
    let listener = restart_listener(address);
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    region_with_peer(&url, 752, None, 3).await;
    result.as_array_mut().unwrap().push(restarted(&url));
    region_with_peer(&url, 752, None, 4).await;
    tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let state = request(&url, "GET", "/pd/api/v1/store/4", "");
            let limits =
                request(&url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"].clone();
            if state.1["store"]["state"] == 2 && limits.get("4").is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_052_lifecycle_and_restart() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_052_differential() {
    let rust = rust_observe().await;
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
    let mut result = observe(&go.url, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    region_with_peer(&go.url, id, None, 3).await;
    result.as_array_mut().unwrap().push(restarted(&go.url));
    go.stop().unwrap();
    println!("Go: {result}\nRust: {rust}");
    assert_eq!(result, rust);
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct StateStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for StateStore {
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
        let mode = self.mode.load(std::sync::atomic::Ordering::SeqCst);
        let target = writes.iter().any(|write| {
            let key = match write {
                Mutation::Put { key, .. } | Mutation::Delete { key } => key,
            };
            if mode == 5 {
                key.ends_with(b"/alloc_id")
            } else if mode == 3 || mode == 4 {
                key.ends_with(b"/config")
            } else {
                key.windows(8).any(|w| w == b"/raft/s/")
            }
        });
        if target {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 | 3 | 5 => writes.push(Mutation::Put {
                    key: b"/task052/failure".to_vec(),
                    value: vec![],
                    lease: Some(i64::MAX),
                }),
                2 | 4 => {
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
async fn task_052_fault_cancel_and_forward() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(StateStore {
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
                cluster_id: 7521,
                member_id: 1,
                name: "leader".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 7521,
        ..Default::default()
    });
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
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
    assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    let read = Read::prefix(
        b"/pd/7521/raft/s/",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(1, Ordering::SeqCst);
    let failed = request(&url, "DELETE", "/pd/api/v1/store/1?force", "");
    assert_eq!(failed.0, 400, "{failed:?}");
    assert!(failed.1.as_str().unwrap().contains("lease"));
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/store/1", "").1["store"]["state"]
            .as_u64()
            .unwrap_or(0),
        0
    );
    let r = client
        .put_store(pdpb::PutStoreRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 7,
                address: "store-7".into(),
                state: 2,
                node_state: 3,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    store.mode.store(1, Ordering::SeqCst);
    let failed = request(&url, "DELETE", "/pd/api/v1/stores/remove-tombstone", "");
    assert_eq!(failed.0, 500, "{failed:?}");
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/store/7", "").1["store"]["state"],
        2
    );
    assert_eq!(
        request(&url, "DELETE", "/pd/api/v1/stores/remove-tombstone", "").0,
        200
    );
    assert_eq!(request(&url, "GET", "/pd/api/v1/store/7", "").0, 404);
    let r = client
        .put_store(pdpb::PutStoreRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 6,
                address: "store-6".into(),
                state: 1,
                node_state: 2,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    // A non-destroying transition from Up records the previous limit.
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"max-replicas":1}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "POST", "/pd/api/v1/store/6/state?state=Up", "").0,
        200
    );
    assert_eq!(request(&url, "DELETE", "/pd/api/v1/store/6", "").0, 200);
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"store-limit":{"6":{"add-peer":77}}}"#
        )
        .0,
        200
    );
    store.mode.store(3, Ordering::SeqCst);
    assert_eq!(
        request(&url, "POST", "/pd/api/v1/store/6/state?state=Up", "").0,
        200
    );
    let limit =
        request(&url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"]["6"].clone();
    assert_eq!(limit["add-peer"].as_f64(), Some(77.0));
    assert_eq!(limit["remove-peer"].as_f64(), Some(15.0));
    store.mode.store(3, Ordering::SeqCst);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7521,
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
            "DELETE",
            "/pd/api/v1/store/1?force=false",
            ""
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/store/1", "").1["store"]["physically_destroyed"],
        true
    );
    assert!(
        request(&url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"]
            .get("1")
            .is_none()
    );
    follower.shutdown().await.unwrap();
    let r = client
        .put_store(pdpb::PutStoreRequest {
            header,
            store: Some(metapb::Store {
                id: 4,
                address: "store-4".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
    let pending_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                "DELETE",
                "-w",
                "\n%{http_code}",
                &format!("{pending_url}/pd/api/v1/store/4?force"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let out = pending.await.unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.ends_with("500"), "{text}");
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}

async fn region_with_peer(url: &str, id: u64, extra: Option<u64>, version: u64) {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let leader = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let mut peers = vec![leader.clone()];
    if version <= 3 {
        for store_id in [4, 5, 8, 9] {
            peers.push(metapb::Peer {
                id: store_id + 100,
                store_id,
                ..Default::default()
            });
        }
    }
    if let Some(store_id) = extra {
        peers.push(metapb::Peer {
            id: store_id + 100,
            store_id,
            ..Default::default()
        });
    }
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 2,
            peers: peers.clone(),
            region_epoch: Some(metapb::RegionEpoch {
                version,
                conf_ver: version,
            }),
            ..Default::default()
        }),
        leader: Some(leader),
        approximate_size: 1024 * 1024,
        ..Default::default()
    })
    .await
    .unwrap();
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let response = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: header.clone(),
                    region_id: 2,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if response
                .region
                .is_some_and(|r| r.peers == peers && r.region_epoch.unwrap().version == version)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    drop(stream);
}

fn restarted(url: &str) -> Value {
    for (id, state, destroyed) in [(4, 1, false), (5, 1, true), (8, 1, true), (9, 1, false)] {
        let value = request(url, "GET", &format!("/pd/api/v1/store/{id}"), "");
        assert_eq!(value.0, 200);
        assert_eq!(value.1["store"]["state"], state);
        assert_eq!(
            value.1["store"]["physically_destroyed"]
                .as_bool()
                .unwrap_or(false),
            destroyed
        );
    }
    assert_eq!(request(url, "GET", "/pd/api/v1/store/7", "").0, 404);
    let response = request(url, "POST", "/pd/api/v1/store/5/state?state=Up", "");
    assert_eq!(response.0, 400);
    json!(["restart", response])
}

// A restart must exercise persisted state, not the platform TCP TIME_WAIT policy.
// REUSEADDR permits rebinding after close; REUSEPORT is deliberately not enabled.
fn restart_listener(address: std::net::SocketAddr) -> tokio::net::TcpListener {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_reuseaddr(true).unwrap();
    socket.bind(address).unwrap();
    socket.listen(128).unwrap()
}
