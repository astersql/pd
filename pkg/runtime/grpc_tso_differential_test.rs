// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_tso_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_classic_tso_differential() {
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
                    cluster_id: 3601,
                    member_id: 1,
                    name: "tso-diff".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let mut rust = PdClient::connect(url.clone()).await.unwrap();
    let expected = scenarios::observe(&mut rust, 3601).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower_node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 3601,
                    member_id: 2,
                    name: "tso-follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower_node.ready().await.unwrap();
    let expected_follower =
        scenarios::follower(&mut PdClient::connect(follower_url).await.unwrap(), 3601).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = Server::start(&binary).unwrap();
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
    assert_eq!(expected, scenarios::observe(&mut client, id).await);
    let last = scenarios::minimum(&mut client, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let next = scenarios::minimum(&mut client, id).await;
    assert!((next.physical, next.logical) > (last.physical, last.logical));
    for address in [&url, &go.url] {
        assert!(
            std::process::Command::new("curl")
                .args(["-fsS", &format!("{address}/pd/api/v1/ping")])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let join = format!("--join={}", go.url);
    let mut follower = Server::start_with_args(
        &binary,
        &["--name=tso-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    assert_eq!(
        expected_follower,
        scenarios::follower(
            &mut PdClient::connect(follower.url.clone()).await.unwrap(),
            id
        )
        .await
    );
    follower.stop().unwrap();
    follower_node.shutdown().await.unwrap();
    println!(
        "classic TSO/GetMinTS shared ordering, batch ranges, overflow, count errors, ignored legacy dc and HTTP ping match real Go; Go restart preserves monotonicity"
    );
    go.stop().unwrap();
    node.shutdown().await.unwrap();
}
