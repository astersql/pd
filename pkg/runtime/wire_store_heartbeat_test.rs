// Copyright 2026 AsterSQL.
use super::{node_test::fixture, *};
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta, StoreState};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_server::store_heartbeat_adapter::StoreHeartbeatService;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_008_behavior() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 808).await;
    let cluster = BasicCluster::new();
    for (id, state, node_state) in [
        (1, StoreState::Up, NodeState::Serving),
        (3, StoreState::Tombstone, NodeState::Removed),
    ] {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            state,
            node_state,
            ..Default::default()
        })));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let registry = Arc::new(prometheus::Registry::new());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 808,
                member_id: 8,
                name: "store-heartbeat".into(),
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
    let request = |id, capacity| pdpb::StoreHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 808,
            ..Default::default()
        }),
        stats: Some(pdpb::StoreStats {
            store_id: id,
            capacity,
            available: capacity / 2,
            ..Default::default()
        }),
        ..Default::default()
    };
    for capacity in [100, 200] {
        let reply = client
            .store_heartbeat(request(1, capacity))
            .await
            .unwrap()
            .into_inner();
        let header = reply.header.unwrap();
        assert_eq!(header.cluster_id, 808);
        assert!(header.error.is_none());
        assert_eq!(
            reply.state,
            astersql_pd_proto::metapb::NodeState::Serving as i32
        );
        let snapshot = host.snapshot().await.unwrap();
        let stats = &snapshot
            .stores
            .iter()
            .find(|(meta, _)| *meta == 1)
            .unwrap()
            .1;
        assert_eq!(stats.capacity, capacity);
        assert_eq!(stats.available, capacity / 2);
    }
    for (id, kind, message) in [
        (999, pdpb::ErrorType::Unknown, "store 999 not found"),
        (3, pdpb::ErrorType::StoreTombstone, "store is tombstone"),
    ] {
        let reply = client
            .store_heartbeat(request(id, 999))
            .await
            .unwrap()
            .into_inner();
        let header = reply.header.unwrap();
        assert_eq!(header.cluster_id, 808);
        let error = header.error.unwrap();
        assert_eq!(error.r#type, kind as i32);
        assert_eq!(error.message, message);
    }
    let mut invalid = request(1, 999);
    invalid.stats = None;
    assert_eq!(
        client.store_heartbeat(invalid).await.unwrap_err().code(),
        tonic::Code::Unknown
    );
    let mut invalid = request(1, 999);
    invalid.header.as_mut().unwrap().cluster_id = 809;
    assert_eq!(
        client.store_heartbeat(invalid).await.unwrap_err().code(),
        tonic::Code::FailedPrecondition
    );
    let snapshot = host.snapshot().await.unwrap();
    assert_eq!(snapshot.stores.len(), 2);
    assert_eq!(
        snapshot
            .stores
            .iter()
            .find(|(meta, _)| *meta == 1)
            .unwrap()
            .1
            .capacity,
        200
    );
    assert_eq!(
        snapshot
            .stores
            .iter()
            .find(|(meta, _)| *meta == 3)
            .unwrap()
            .1
            .capacity,
        0
    );
    drop(client);
    cancel.cancel();
    node.wait().await.unwrap();
    assert!(
        host.store_heartbeat(astersql_pd_core::StoreStats {
            store_id: 1,
            capacity: 999,
            ..Default::default()
        })
        .await
        .is_err()
    );
    assert!(host.snapshot().await.is_err());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    assert!(registry.gather().is_empty());
    assert_eq!(Arc::strong_count(&store), 1);
}
