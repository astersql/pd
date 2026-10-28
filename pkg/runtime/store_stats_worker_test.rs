// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_core::{StoreInfo, StoreMeta};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use std::time::Duration;

async fn report(client: &mut PdClient<tonic::transport::Channel>, id: u64) {
    let peer = metapb::Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let request = pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 1,
            peers: vec![peer.clone()],
            region_epoch: Some(metapb::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 5 * 1048576,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(request.clone()).await.unwrap();
    tx.send(request.clone()).await.unwrap();
    let mut stale = request;
    stale
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 1;
    stale.approximate_size = 99 * 1048576;
    tx.send(stale).await.unwrap();
    let mut output = tokio::time::timeout(
        Duration::from_secs(5),
        client.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await
    .unwrap()
    .unwrap()
    .into_inner();
    let response = tokio::time::timeout(Duration::from_secs(5), output.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let error = response.header.unwrap().error.unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert!(error.message.contains("stale"));
    tokio::time::sleep(Duration::from_millis(200)).await;
}
fn counters(store: &astersql_pd_core::StoreInfo) -> [i64; 4] {
    [
        store.leader_count(),
        store.region_count(),
        store.leader_size(),
        store.region_size(),
    ]
}
async fn rust_scenario() -> [i64; 4] {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 513).await;
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 513,
                member_id: 13,
                name: "store-stats".into(),
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
    let mut client = PdClient::connect(url).await.unwrap();
    report(&mut client, 513).await;
    let retained = host.state.upgrade().unwrap();
    let actual = counters(
        &retained
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap(),
    );
    assert_eq!(actual, [1, 1, 5, 5]);
    let endpoint = StorageEndpoint::new(store.clone());
    assert!(
        endpoint
            .load_region(&ClusterPaths::new(513), 1)
            .await
            .unwrap()
            .is_some()
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        counters(
            &retained
                .lock()
                .unwrap()
                .cluster
                .stores
                .get_store(1)
                .unwrap()
        ),
        actual
    );
    node.shutdown().await.unwrap();
    retained
        .lock()
        .unwrap()
        .cluster
        .stores
        .patch_store(1, |s| s.set_region_count(77));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        retained
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .region_count(),
        77
    );
    assert!(registry.gather().is_empty());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    drop((client, endpoint, retained));
    assert_eq!(Arc::strong_count(&store), 1);
    actual
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_013_runtime_heartbeat_and_cancel() {
    rust_scenario().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go PD binary and read-only supervisor"]
async fn readonly_store_stats_differential() {
    use astersql_pd_compat_tests::harness::Server;
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
    let result = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
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
    assert!(result.header.unwrap().error.is_none());
    report(&mut client, id).await;
    let (status, body) = server.get("/pd/api/v1/store/1").unwrap();
    assert_eq!(status, 200);
    let s = &body["status"];
    let actual = [
        s["leader_count"].as_i64().unwrap(),
        s["region_count"].as_i64().unwrap(),
        s["leader_size"].as_i64().unwrap(),
        s["region_size"].as_i64().unwrap(),
    ];
    assert_eq!(actual, rust);
    println!("task013 Go HTTP/Rust shared counters {actual:?}");
    server.stop().unwrap();
}
