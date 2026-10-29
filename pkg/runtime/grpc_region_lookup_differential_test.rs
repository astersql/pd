// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_region_lookup_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = vec![];
    let mut rust = vec![];
    for member_id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    crate::NodeConfig {
                        cluster_id: 2301,
                        member_id,
                        name: format!("region{member_id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://peer{member_id}"),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        rust.push(PdClient::connect(url).await.unwrap());
    }
    scenarios::prepare(&mut rust[0], 2301).await;
    let mut expected = vec![];
    for (i, client) in rust.iter_mut().enumerate() {
        expected.push(scenarios::observe(client, 2301, i == 1).await);
    }
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut leader = Server::start(&binary).unwrap();
    leader.ready().unwrap();
    let join = format!("--join={}", leader.url);
    let mut follower = Server::start_with_args(
        &binary,
        &["--name=region-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    scenarios::prepare(&mut client, id).await;
    let mut actual = vec![scenarios::observe(&mut client, id, false).await];
    let mut client = PdClient::connect(follower.url.clone()).await.unwrap();
    actual.push(scenarios::observe(&mut client, id, true).await);
    assert_eq!(expected, actual);
    println!(
        "44 full protobuf responses per role match Go; leader/follower boundaries, gaps, missing IDs, bucket flags and admission verified"
    );
    follower.stop().unwrap();
    leader.stop().unwrap();
    for node in nodes.iter_mut().rev() {
        node.shutdown().await.unwrap();
    }
}
