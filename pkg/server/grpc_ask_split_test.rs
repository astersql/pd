// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_026_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2600,
                member_id: 1,
                name: "split".into(),
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
    let response = client
        .ask_split(pdpb::AskSplitRequest {
            header: super::grpc_region_lookup_scenarios_test::header(2600),
            ..Default::default()
        })
        .await;
    assert!(
        response.is_ok(),
        "AskSplit must serve protocol responses: {response:?}"
    );
    assert_eq!(
        response
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2600).await;
    super::grpc_ask_split_scenarios_test::observe_version_gate(&mut client, 2600).await;
    let before = store
        .read(Read::prefix(b"/pd/2600/raft/r/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    super::grpc_ask_split_scenarios_test::observe(&mut client, 2600).await;
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(
        store
            .read(Read::prefix(b"/pd/2600/raft/r/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 2600,
                    member_id: 2,
                    name: "split-follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    let mut follower_client = PdClient::connect(follower_url).await.unwrap();
    super::grpc_ask_split_scenarios_test::observe_follower(&mut follower_client, 2600).await;
    follower.shutdown().await.unwrap();
    let boundary = store
        .read(Read::prefix(
            b"/pd/2600/alloc_id",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert!(!boundary.is_empty());
    let last = u64::from_be_bytes(boundary[0].value.as_slice().try_into().unwrap());
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 2600,
                member_id: 1,
                name: "split".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    let region = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: super::grpc_region_lookup_scenarios_test::header(2600),
            region_id: 10,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .region;
    let r = client
        .ask_batch_split(pdpb::AskBatchSplitRequest {
            header: super::grpc_region_lookup_scenarios_test::header(2600),
            region,
            split_count: 1,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    assert!(r.ids[0].new_region_id > last);
    node.shutdown().await.unwrap();
}
