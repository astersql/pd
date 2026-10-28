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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_009_behavior() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 505).await;
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
    let mut stale = report.clone();
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
    let error = tokio::time::timeout(Duration::from_secs(5), output.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let header = error.header.unwrap();
    assert_eq!(header.cluster_id, 505);
    let error = header.error.unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert!(error.message.starts_with("region is stale"));
    assert_eq!(
        host.snapshot().await.unwrap().regions[0]
            .stats()
            .approximate_size,
        2
    );
    assert_eq!(
        storage.load_region(&paths, 7).await.unwrap(),
        Some(snapshot.regions[0].meta().clone())
    );
    assert_eq!(
        host.snapshot().await.unwrap().regions[0]
            .meta()
            .epoch
            .as_ref()
            .unwrap()
            .version,
        2
    );
    // A valid report after the error must still advance the same stream and indexes.
    let mut next = report;
    next.region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 3;
    next.approximate_size = 3 * 1048576;
    tx.send(next).await.unwrap();
    drop(tx);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), output.message())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = host.snapshot().await.unwrap();
            let persisted = storage.load_region(&paths, 7).await.unwrap().unwrap();
            if snapshot.store_regions[0].1.region_size == 3
                && persisted.epoch.as_ref().unwrap().version == 3
            {
                assert_eq!(snapshot.store_regions[0].1.region, 1);
                assert_eq!(snapshot.store_regions[0].1.leader, 1);
                assert_eq!(snapshot.store_regions[0].1.leader_size, 3);
                assert_eq!(snapshot.regions[0].stats().approximate_size, 3);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut output = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let health = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/health")])
        .output()
        .unwrap();
    assert!(health.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&health.stdout).unwrap()[0]["health"],
        true
    );
    // Host cancellation must terminate an open bidirectional stream.
    parent.cancel();
    tokio::time::timeout(Duration::from_secs(5), node.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), output.message())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    assert!(host.snapshot().await.is_err());
    drop(storage);
    assert_eq!(
        Arc::strong_count(&store),
        1,
        "closed node and read handles must release MetaStore"
    );
    assert!(registry.gather().is_empty());
    drop((tx, output, client));
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}
