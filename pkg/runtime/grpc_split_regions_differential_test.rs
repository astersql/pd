// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_split_regions_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_split_regions_differential() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                crate::NodeConfig {
                    cluster_id: 2900,
                    member_id: 1,
                    name: "split-regions".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let mut rust = PdClient::connect(url).await.unwrap();
    let unboot = scenarios::unbootstrapped(&mut rust, 2900).await;
    prepare::prepare(&mut rust, 2900).await;
    let expected = scenarios::observe(&mut rust, 2900).await;
    let follower_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", follower_listener.local_addr().unwrap());
    let mut follower_node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 2900,
                    member_id: 2,
                    name: "split-regions-follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                follower_listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower_node.ready().await.unwrap();
    let mut follower = PdClient::connect(follower_url).await.unwrap();
    let follower_expected = scenarios::unbootstrapped(&mut follower, 2900).await;
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
    assert_eq!(unboot, scenarios::unbootstrapped(&mut client, id).await);
    prepare::prepare(&mut client, id).await;
    let actual = scenarios::observe(&mut client, id).await;
    assert_eq!(expected, actual);
    println!("split real heartbeat completion matches Go: {actual}");
    let join = format!("--join={}", go.url);
    let mut go_follower = astersql_pd_compat_tests::harness::Server::start_with_args(
        &binary,
        &["--name=split-regions-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    go_follower.ready().unwrap();
    let mut follower = PdClient::connect(go_follower.url.clone()).await.unwrap();
    assert_eq!(
        follower_expected,
        scenarios::unbootstrapped(&mut follower, id).await
    );
    go_follower.stop().unwrap();
    follower_node.shutdown().await.unwrap();
    go.stop().unwrap();
    node.shutdown().await.unwrap();
}
