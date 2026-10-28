// Copyright 2026 AsterSQL.
use super::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_core::{NodeState, RegionEpoch, RegionMeta, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn config(address: std::net::SocketAddr) -> NodeConfig {
    NodeConfig {
        cluster_id: 606,
        member_id: 6,
        name: "restore".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://127.0.0.1:2380".into(),
    }
}
fn header() -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: 606,
        ..Default::default()
    })
}
async fn seed(store: Arc<dyn MetaStore>, cluster_id: u64) {
    crate::node_test::seed_bootstrap(store.clone(), cluster_id).await;
    let endpoint = StorageEndpoint::new(store);
    let paths = ClusterPaths::new(cluster_id);
    endpoint
        .save_store_meta(
            &paths,
            &StoreMeta {
                id: 1,
                address: "127.0.0.1:29999".into(),
                node_state: NodeState::Serving,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    endpoint
        .save_store_weight(&paths, 1, 2.5, 3.5)
        .await
        .unwrap();
    endpoint
        .save_region(
            &paths,
            &RegionMeta {
                id: 7,
                peers: vec![astersql_pd_core::Peer {
                    id: 2,
                    store_id: 1,
                    ..Default::default()
                }],
                epoch: Some(RegionEpoch {
                    version: 3,
                    conf_ver: 1,
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    endpoint.save_config(&paths, &serde_json::json!({"replication":{"max-replicas":5,"enable-placement-rules":"false"},"pd-server":{"key-type":"raw","use-region-storage":"false"}})).await.unwrap();
}
async fn rust_scenario() -> serde_json::Value {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    seed(store.clone(), 606).await;
    let paths = ClusterPaths::new(606);
    let endpoint = StorageEndpoint::new(store.clone());
    let persisted = endpoint.load_region(&paths, 7).await.unwrap().unwrap();
    let registry = Arc::new(prometheus::Registry::new());
    let parent = CancellationToken::new();
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    for _ in 0..2 {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let config = config(address);
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await
            .unwrap();
        node.ready().await.unwrap();
        let host = node.cluster();
        let recovered_store = astersql_pd_server::ClusterReadService::store(host.as_ref(), 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovered_store.leader_weight(), 2.5);
        assert_eq!(recovered_store.region_weight(), 3.5);
        let snapshot = host.snapshot().await.unwrap();
        assert_eq!(
            snapshot.stores.len(),
            1,
            "ready must include durable stores"
        );
        assert_eq!(
            snapshot.regions.len(),
            1,
            "ready must include durable regions"
        );
        assert_eq!(snapshot.regions[0].meta(), &persisted);
        assert!(snapshot.regions[0].leader().is_none(), "leader is volatile");
        assert_eq!(
            host.restored_config()
                .unwrap()
                .config
                .replication
                .max_replicas,
            5
        );
        assert_eq!(
            host.restored_config().unwrap().config.pd_server.key_type,
            "raw"
        );
        let mut client = PdClient::connect(config.client_url.clone()).await.unwrap();
        let remote_store = client
            .get_store(pdpb::GetStoreRequest {
                header: header(),
                store_id: 1,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(remote_store.header.unwrap().error.is_none());
        assert_eq!(remote_store.store.unwrap().address, "127.0.0.1:29999");
        let remote = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header(),
                region_id: 7,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(remote.region.unwrap().region_epoch.unwrap().version, 3);
        assert!(remote.leader.is_none());
        let absent = client
            .get_store(pdpb::GetStoreRequest {
                header: header(),
                store_id: 99,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(absent.header.unwrap().error.is_some());
        let missing = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header(),
                region_id: 99,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(missing.region.is_none());
        let response = std::process::Command::new("curl")
            .args(["-fsS", &format!("{}/pd/api/v1/health", config.client_url)])
            .output()
            .unwrap();
        assert!(response.status.success());
        drop(client);
        node.shutdown().await.unwrap();
        assert!(host.snapshot().await.is_err());
        assert!(registry.gather().is_empty());
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
    assert!(!parent.is_cancelled());
    assert_eq!(
        endpoint.load_region(&paths, 7).await.unwrap(),
        Some(persisted)
    );
    drop(endpoint);
    assert_eq!(Arc::strong_count(&store), 1);
    serde_json::json!({"store":1,"address":"127.0.0.1:29999","region":7,"version":3,"leader_absent":true,"replicas":5,"key_type":"raw","healthy":true})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_006_behavior() {
    rust_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_006_corrupt_data_releases_resources() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let paths = ClusterPaths::new(606);
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    for (key, value) in [
        (paths.store_path(1), vec![255]),
        (paths.region_path(7), vec![255]),
        (paths.config_path(), b"{".to_vec()),
        (
            paths.config_path(),
            br#"{"pd-server":{"key-type":"invalid"}}"#.to_vec(),
        ),
    ] {
        store
            .transact(vec![], vec![Mutation::put(key.as_bytes(), &value)])
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let parent = CancellationToken::new();
        let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
            .start(store.clone(), config(address), listener, registry.clone())
            .await;
        assert!(
            result.is_err(),
            "corrupt persisted data must prevent startup: {key}"
        );
        assert!(!parent.is_cancelled());
        assert!(registry.gather().is_empty());
        assert_eq!(Arc::strong_count(&store), 1);
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        store
            .transact(
                vec![],
                vec![Mutation::Delete {
                    key: key.as_bytes().to_vec(),
                }],
            )
            .await
            .unwrap();
        // Same identity succeeds immediately: failure released leadership and listener.
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent)
            .start(store.clone(), config(address), listener, registry.clone())
            .await
            .unwrap();
        node.ready().await.unwrap();
        node.shutdown().await.unwrap();
        assert!(registry.gather().is_empty());
    }
}

// Fault injection at the actual recovery read boundary. Every unaffected operation
// uses the same live etcd backend, including member transactions and lease cleanup.
struct RecoveryGate {
    inner: Arc<dyn MetaStore>,
    entered: tokio::sync::Notify,
    fail: bool,
}
#[async_trait::async_trait]
impl MetaStore for RecoveryGate {
    async fn read(&self, request: Read) -> Result<Snapshot> {
        if request.start.as_deref() == Some(ClusterPaths::new(606).config_path().as_bytes())
            || request.prefix == ClusterPaths::new(606).config_path().as_bytes()
        {
            self.entered.notify_one();
            if self.fail {
                return Err(StoreError::Unavailable);
            }
            return std::future::pending().await;
        }
        self.inner.read(request).await
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(&self, prefix: Vec<u8>, start_revision: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> Result<()> {
        self.inner.compact(revision).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_006_cancel_and_read_failure_release_resources() {
    for mode in ["cancel", "read-error", "lease-loss"] {
        let fail = mode == "read-error";
        let fixture = fixture::Fixture::start();
        let store = Arc::new(fixture.store().await);
        let gate = Arc::new(RecoveryGate {
            inner: store.clone(),
            entered: tokio::sync::Notify::new(),
            fail,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let registry = Arc::new(prometheus::Registry::new());
        let parent = CancellationToken::new();
        let builder = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone());
        let task =
            tokio::spawn(builder.start(gate.clone(), config(address), listener, registry.clone()));
        tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
            .await
            .unwrap();
        match mode {
            "cancel" => parent.cancel(),
            "lease-loss" => {
                let leader = store
                    .read(Read::prefix(b"/pd/606/leader", Consistency::Linearizable))
                    .await
                    .unwrap();
                store
                    .revoke(leader.entries[0].lease.unwrap())
                    .await
                    .unwrap();
            }
            _ => {}
        }
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
        assert!(registry.gather().is_empty());
        assert_eq!(
            Arc::strong_count(&gate),
            1,
            "failed startup retains no backend handles"
        );
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(store.clone(), config(address), listener, registry.clone())
                .await
                .unwrap();
        node.ready().await.unwrap();
        node.shutdown().await.unwrap();
        assert!(registry.gather().is_empty());
    }
}

/// Run only under the hashing/process-group supervisor with the existing Go binary.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_cluster_restore_differential() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle};
    use astersql_pd_storage_kv::EtcdMetaStore;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    let work = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_WORK_DIR").unwrap());
    readonly_oracle::validate(
        "go",
        std::path::Path::new(&go),
        None,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/rust-compat/fixtures/health-member.json"),
    )
    .unwrap();
    let rust = rust_scenario().await;
    let file = work.join("cluster-restore-go.toml");
    std::fs::write(&file, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let arg = format!("--config={}", file.display());
    let mut server = Server::start_with_args(std::path::Path::new(&go), &[&arg]).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let peer = astersql_pd_proto::metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(astersql_pd_proto::metapb::Region {
                id: 7,
                peers: vec![peer],
                region_epoch: Some(astersql_pd_proto::metapb::RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let backend = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
            .await
            .unwrap(),
    );
    // Persist through the same canonical schema; reboot must consume disk, not old cache.
    seed(backend.clone(), cluster_id).await;
    drop((client, backend));
    server
        .restart_with_args(std::path::Path::new(&go), &[&arg])
        .unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let remote_store = client
        .get_store(pdpb::GetStoreRequest {
            header: header.clone(),
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(remote_store.header.unwrap().error.is_none());
    let remote_region = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: header.clone(),
            region_id: 7,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let meta = remote_region.region.unwrap();
    let (status, config) = server.get("/pd/api/v1/config").unwrap();
    assert_eq!(status, 200);
    let (_, health) = server.get("/pd/api/v1/health").unwrap();
    let observed = serde_json::json!({"store":remote_store.store.as_ref().unwrap().id,"address":remote_store.store.unwrap().address,"region":meta.id,"version":meta.region_epoch.unwrap().version,"leader_absent":remote_region.leader.is_none(),"replicas":config["replication"]["max-replicas"],"key_type":config["pd-server"]["key-type"],"healthy":health[0]["health"]});
    println!("Go={observed}; Rust={rust}");
    assert_eq!(observed, rust);
    // Go rejects the cluster cache on a malformed durable region; its generic
    // etcd health endpoint is separate from cluster readiness.
    let backend = EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
        .await
        .unwrap();
    let paths = ClusterPaths::new(cluster_id);
    backend
        .transact(
            vec![],
            vec![Mutation::put(paths.region_path(7).as_bytes(), &[255])],
        )
        .await
        .unwrap();
    drop((backend, client));
    server
        .restart_with_args(std::path::Path::new(&go), &[&arg])
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if server.logs().contains("failed to load cluster info") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(server.logs().contains("ErrProtoUnmarshal"));
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let broken = tokio::time::timeout(
        Duration::from_secs(3),
        client.get_store(pdpb::GetStoreRequest {
            header,
            store_id: 1,
        }),
    )
    .await
    .unwrap();
    match broken {
        Ok(response) => assert!(response.into_inner().header.unwrap().error.is_some()),
        Err(status) => println!("Go corrupt cluster read rejected: {}", status.code()),
    }
    println!(
        "Go corrupt region refuses cluster reads; Rust corrupt region refuses start; both shutdown cleanly"
    );
    drop(client);
    server.stop().unwrap();
}
