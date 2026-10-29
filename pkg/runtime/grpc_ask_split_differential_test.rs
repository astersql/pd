// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_ask_split_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_ask_split_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                crate::NodeConfig {
                    cluster_id: 2601,
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
    let mut rust = PdClient::connect(url).await.unwrap();
    prepare::prepare(&mut rust, 2601).await;
    let version_expected = scenarios::observe_version_gate(&mut rust, 2601).await;
    let expected = scenarios::observe(&mut rust, 2601).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower_node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 2601,
                    member_id: 2,
                    name: "split-follower".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower_node.ready().await.unwrap();
    let mut follower_client = PdClient::connect(url).await.unwrap();
    let follower_expected = scenarios::observe_follower(&mut follower_client, 2601).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut leader = Server::start(&binary).unwrap();
    leader.ready().unwrap();
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    prepare::prepare(&mut client, id).await;
    let join = format!("--join={}", leader.url);
    let mut go_follower = Server::start_with_args(
        &binary,
        &["--name=split-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    go_follower.ready().unwrap();
    let mut follower_client = PdClient::connect(go_follower.url.clone()).await.unwrap();
    assert_eq!(
        follower_expected,
        scenarios::observe_follower(&mut follower_client, id).await
    );
    assert_eq!(
        version_expected,
        scenarios::observe_version_gate(&mut client, id).await
    );
    let actual = scenarios::observe(&mut client, id).await;
    assert_eq!(expected, actual);
    println!(
        "AskSplit/AskBatchSplit: 19 full normalized leader responses match Go; 148 unique IDs per implementation; stale requests consume none"
    );
    go_follower.stop().unwrap();
    follower_node.shutdown().await.unwrap();
    leader.stop().unwrap();
    node.shutdown().await.unwrap();
}
