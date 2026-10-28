// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_core::{StoreInfo, StoreMeta, StoreState};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use std::time::Duration;

async fn observe(
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
    storage: &StorageEndpoint,
) -> Vec<i32> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    loop {
        let mut states = Vec::new();
        for store_id in [2, 3] {
            let response = client
                .get_store(pdpb::GetStoreRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: id,
                        ..Default::default()
                    }),
                    store_id,
                })
                .await
                .unwrap()
                .into_inner();
            assert!(response.header.unwrap().error.is_none());
            states.push(response.store.unwrap().node_state);
        }
        if states
            == [
                metapb::NodeState::Serving as i32,
                metapb::NodeState::Removed as i32,
            ]
        {
            for (store_id, expected) in [(2, NodeState::Serving), (3, NodeState::Removed)] {
                assert_eq!(
                    storage
                        .load_store_meta(&ClusterPaths::new(id), store_id)
                        .await
                        .unwrap()
                        .unwrap()
                        .node_state,
                    expected
                );
            }
            return states;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker did not publish store states: {states:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn rust_scenario() -> Vec<i32> {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 512).await;
    let storage = StorageEndpoint::new(store.clone());
    let cluster = BasicCluster::new();
    for (id, node_state, state) in [
        (2, NodeState::Preparing, StoreState::Up),
        (3, NodeState::Removing, StoreState::Offline),
    ] {
        let meta = StoreMeta {
            id,
            node_state,
            state,
            last_heartbeat: now().1,
            ..Default::default()
        };
        storage
            .save_store_meta(&ClusterPaths::new(512), &meta)
            .await
            .unwrap();
        cluster.stores.put_store(Arc::new(StoreInfo::new(meta)));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 512,
                member_id: 12,
                name: "store-state".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let states = observe(&mut client, 512, &storage).await;
    assert!(host.store_state_check_errors().is_empty());
    let retained = host.state.upgrade().unwrap();
    // Put a pending transition into the canonical cache immediately before cancel.
    retained
        .lock()
        .unwrap()
        .cluster
        .stores
        .patch_store(2, |s| s.edit_meta(|m| m.node_state = NodeState::Preparing));
    node.shutdown().await.unwrap();
    assert!(host.snapshot().await.is_err());
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    tokio::time::sleep(Duration::from_secs(11)).await;
    assert_eq!(
        retained
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(2)
            .unwrap()
            .meta()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    assert_eq!(
        storage
            .load_store_meta(&ClusterPaths::new(512), 2)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    drop((storage, retained, client));
    assert_eq!(Arc::strong_count(&store), 1);
    states
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_012_runtime_tick_and_cancel() {
    rust_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_store_state_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_storage_kv::EtcdMetaStore;
    let rust = rust_scenario().await;
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap();
    let mut server = Server::start(std::path::Path::new(&go)).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29999".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 1,
                peers: vec![metapb::Peer {
                    id: 1,
                    store_id: 1,
                    ..Default::default()
                }],
                region_epoch: Some(metapb::RegionEpoch {
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
    for store_id in [2, 3, 4] {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("127.0.0.1:{}", 30000 + store_id),
                    version: "8.5.0".into(),
                    last_heartbeat: now().1,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
    }
    let response = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-X",
            "DELETE",
            &format!("{}/pd/api/v1/store/3", server.url),
        ])
        .output()
        .unwrap();
    assert!(
        response.status.success(),
        "{}",
        String::from_utf8_lossy(&response.stderr)
    );
    let backend = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
            .await
            .unwrap(),
    );
    let states = observe(&mut client, id, &StorageEndpoint::new(backend)).await;
    assert_eq!(states, rust);
    let (status, health) = server.get("/pd/api/v1/health").unwrap();
    assert_eq!(status, 200);
    assert_eq!(health[0]["health"], true);
    println!("task012 dual lifecycle states {states:?}; persisted promotion and burial match");
    server.stop().unwrap();
}

#[tokio::test]
async fn task_012_default_preparing_config() {
    let fixture = fixture::Fixture::start();
    let (_, config, restored) = crate::cluster_restore::restore(
        BasicCluster::new(),
        Arc::new(fixture.store().await),
        &NodeConfig {
            cluster_id: 513,
            member_id: 13,
            name: "store-defaults".into(),
            client_url: "http://127.0.0.1:12379".into(),
            peer_url: "http://127.0.0.1:12380".into(),
        },
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(restored.is_none());
    assert_eq!(
        config.schedule.max_store_preparing_time.as_nanos(),
        48 * 3600 * 1_000_000_000,
        "a new host must use Go's 48-hour preparing default"
    );
    assert_eq!(config.replication.max_replicas, 3);
}
