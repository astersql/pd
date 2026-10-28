// Copyright 2026 AsterSQL.
use super::node_test::fixture;
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn rust_scenario() -> serde_json::Value {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let parent = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let config = NodeConfig {
        cluster_id: 505,
        member_id: 5,
        name: "shared".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .with_cluster(cluster)
        .start(store.clone(), config.clone(), listener, registry.clone())
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 505,
        ..Default::default()
    });
    let result = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 999,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await;
    assert!(
        result.is_ok(),
        "Application must expose the canonical store state service: {result:?}"
    );
    assert!(result.unwrap().into_inner().header.unwrap().error.is_some());
    let result = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 100,
                available: 60,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
    assert_eq!(host.snapshot().await.unwrap().stores[0].1.capacity, 100);
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let report = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 7,
            peers: vec![peer.clone()],
            region_epoch: Some(metapb::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 2097152,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    tx.send(report.clone()).await.unwrap();
    let storage = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(505);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if storage.load_region(&paths, 7).await.unwrap().is_some() {
                break;
            }
            // The receive stream drives admission; successful reports have no ack.
            tokio::select! {
                message = output.message() => panic!("unexpected ack: {message:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
    })
    .await
    .unwrap();
    let snapshot = host.snapshot().await.unwrap();
    assert_eq!(snapshot.regions.len(), 1);
    assert_eq!(snapshot.regions[0].meta().id, 7);
    assert_eq!(snapshot.regions[0].stats().approximate_size, 2);
    assert_eq!(
        storage.load_region(&paths, 7).await.unwrap(),
        Some(snapshot.regions[0].meta().clone())
    );
    let mut stale = report;
    stale
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 1;
    tx.send(stale).await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), output.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let error = error.header.unwrap().error.unwrap();
    let stale_kind = error.r#type;
    let stale_message = error.message.starts_with("region is stale");
    assert_eq!(stale_kind, pdpb::ErrorType::Unknown as i32);
    assert!(stale_message);
    assert_eq!(
        host.snapshot().await.unwrap().regions[0]
            .meta()
            .epoch
            .as_ref()
            .unwrap()
            .version,
        2
    );
    let health = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/health")])
        .output()
        .unwrap();
    assert!(health.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&health.stdout).unwrap()[0]["health"],
        true
    );
    drop((tx, output, client));
    node.shutdown().await.unwrap();
    assert!(host.snapshot().await.is_err());
    drop(storage);
    assert_eq!(
        Arc::strong_count(&store),
        1,
        "closed node and read handles must release MetaStore"
    );
    assert!(registry.gather().is_empty());
    assert!(!parent.is_cancelled());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    // Same host registry and identity may be restarted after all workers join.
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut again = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .start(store, config, listener, registry.clone())
        .await
        .unwrap();
    again.ready().await.unwrap();
    parent.cancel();
    tokio::time::timeout(Duration::from_secs(5), again.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(registry.gather().is_empty());
    serde_json::json!({"capacity": snapshot.stores[0].1.capacity,
        "region": snapshot.regions[0].meta().id,
        "version": snapshot.regions[0].meta().epoch.as_ref().unwrap().version,
        "size": snapshot.regions[0].stats().approximate_size,
        "persisted": true, "missing_store_error": true, "stale_error": true, "stale_kind": stale_kind, "stale_message": stale_message, "healthy": true})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_005_behavior() {
    rust_scenario().await;
}

/// Invoked by the read-only supervisor, which hashes Go inputs and cleans process groups.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and the read-only supervisor"]
async fn readonly_cluster_host_differential() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle};
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use astersql_pd_storage_kv::EtcdMetaStore;
    use prost::Message;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    assert!(std::env::var_os("PD_COMPAT_WORK_DIR").is_some());
    readonly_oracle::validate(
        "go",
        std::path::Path::new(&go),
        None,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/rust-compat/fixtures/health-member.json"),
    )
    .unwrap();
    let rust = rust_scenario().await;
    let config_path = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_WORK_DIR").unwrap())
        .join("cluster-host-go.toml");
    std::fs::write(&config_path, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let mut server = Server::start_with_args(
        std::path::Path::new(&go),
        &[&format!("--config={}", config_path.display())],
    )
    .unwrap();
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
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let initial = metapb::Region {
        id: 7,
        peers: vec![peer.clone()],
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    };
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(initial.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let missing = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 999,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .error
        .is_some();
    client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 100,
                available: 60,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let capacity = client
        .get_store(pdpb::GetStoreRequest {
            header: header.clone(),
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner()
        .stats
        .unwrap()
        .capacity;
    let mut report = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(initial),
        leader: Some(peer),
        approximate_size: 2097152,
        ..Default::default()
    };
    report
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 2;
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(report.clone()).await.unwrap();
    report
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 1;
    tx.send(report).await.unwrap();
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let backend = EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
        .await
        .unwrap();
    let key = format!("/pd/{cluster_id}/raft/r/00000000000000000007");
    let persisted = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let read = backend
                .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
                .await
                .unwrap();
            if let Some(entry) = read.entries.first() {
                let region = metapb::Region::decode(entry.value.as_slice()).unwrap();
                if region.region_epoch.as_ref().unwrap().version == 2 {
                    break region;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let stale = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = output.message().await.unwrap().unwrap();
            if let Some(error) = response.header.and_then(|h| h.error) {
                break error;
            }
        }
    })
    .await
    .unwrap();
    let (status, region) = server.get("/pd/api/v1/region/id/7").unwrap();
    assert_eq!(status, 200);
    let (status, health) = server.get("/pd/api/v1/health").unwrap();
    assert_eq!(status, 200);
    let observed = serde_json::json!({"capacity": capacity, "region": persisted.id,
        "version": persisted.region_epoch.unwrap().version, "size": region["approximate_size"],
        "persisted": true, "missing_store_error": missing, "stale_error": true,
        "stale_kind": stale.r#type, "stale_message": stale.message.starts_with("region is stale"),
        "healthy": health[0]["health"]});
    println!("Go={observed}; Rust={rust}");
    assert_eq!(observed, rust);
    drop((tx, output, client, backend));
    server.stop().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_005_registration_failure_releases_leadership() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let mut existing =
        astersql_pd_statistics::HotStat::new(&tokio::runtime::Handle::current(), &registry, 8)
            .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = NodeConfig {
        cluster_id: 506,
        member_id: 6,
        name: "failure".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store.clone(), config.clone(), listener, registry.clone())
        .await;
    assert!(
        result.is_err(),
        "bucket registration conflict must reject startup"
    );
    existing.snapshot(-20, vec![]).await.unwrap();
    assert!(
        registry
            .gather()
            .iter()
            .all(|family| family.name() != "pd_rust_requests_total")
    );
    existing.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store, config, listener, registry.clone())
        .await
        .unwrap();
    node.ready().await.unwrap();
    node.shutdown().await.unwrap();
    assert!(registry.gather().is_empty());
}
