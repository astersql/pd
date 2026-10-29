// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_025_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2500,
                member_id: 1,
                name: "query".into(),
                client_url: url.clone(),
                peer_url: "http://peer1".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(pdpb::QueryRegionRequest {
        header: super::grpc_region_lookup_scenarios_test::header(2500),
        ..Default::default()
    })
    .await
    .unwrap();
    let result = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await;
    assert!(
        result.is_ok(),
        "QueryRegion must accept the stream: {result:?}"
    );
    let mut stream = result.unwrap().into_inner();
    let r = tokio::time::timeout(Duration::from_secs(10), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        r.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    drop(tx);
    assert!(stream.message().await.unwrap().is_none());
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2500).await;
    super::grpc_region_query_scenarios_test::observe(&mut client, 2500, false).await;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let before = store
        .read(Read::prefix(b"/pd/2500/raft/r/", Consistency::Linearizable))
        .await
        .unwrap();
    // A stream opened before a heartbeat must observe the new cache on its next request.
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut stream = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let request = pdpb::QueryRegionRequest {
        header: super::grpc_region_lookup_scenarios_test::header(2500),
        ids: vec![10],
        ..Default::default()
    };
    tx.send(request.clone()).await.unwrap();
    let old = stream
        .message()
        .await
        .unwrap()
        .unwrap()
        .regions_by_id
        .remove(&10)
        .unwrap();
    let mut region = old.region.clone().unwrap();
    region.region_epoch.as_mut().unwrap().version += 1;
    let heartbeat = client
        .region_heartbeat(tokio_stream::iter([pdpb::RegionHeartbeatRequest {
            header: super::grpc_region_lookup_scenarios_test::header(2500),
            region: Some(region.clone()),
            leader: old.leader,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let r = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: request.header.clone(),
                    region_id: 10,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if r.region == Some(region.clone()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(heartbeat);
    tx.send(request.clone()).await.unwrap();
    let fresh = stream.message().await.unwrap().unwrap();
    assert_eq!(fresh.regions_by_id[&10].region, Some(region));
    drop(tx);
    assert!(stream.message().await.unwrap().is_none());
    let stable = store
        .read(Read::prefix(b"/pd/2500/raft/r/", Consistency::Linearizable))
        .await
        .unwrap();
    let mut query = client
        .query_region(tokio_stream::iter([request]))
        .await
        .unwrap()
        .into_inner();
    assert!(query.message().await.unwrap().is_some());
    assert!(query.message().await.unwrap().is_none());
    let after = store
        .read(Read::prefix(b"/pd/2500/raft/r/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(stable.entries, after.entries);
    assert!(!before.entries.is_empty());
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    node.shutdown().await.unwrap();
}
