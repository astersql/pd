// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_unsupported_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_deprecated_rpc_differential() {
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
                    cluster_id: 3701,
                    member_id: 1,
                    name: "deprecated-diff".into(),
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
    scenarios::observe(&mut rust, 3701).await;
    assert!(
        rust.get_min_ts(pdpb::GetMinTsRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 3701,
                ..Default::default()
            })
        })
        .await
        .unwrap()
        .into_inner()
        .timestamp
        .unwrap()
        .physical
            < i64::MAX
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower_node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                crate::NodeConfig {
                    cluster_id: 3701,
                    member_id: 2,
                    name: "deprecated-follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower_node.ready().await.unwrap();
    scenarios::observe(
        &mut PdClient::connect(follower_url.clone()).await.unwrap(),
        3701,
    )
    .await;
    let status = PdClient::connect(follower_url.clone())
        .await
        .unwrap()
        .get_min_ts(pdpb::GetMinTsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unavailable);
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
    scenarios::observe(&mut client, id).await;
    let minimum = client
        .get_min_ts(pdpb::GetMinTsRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner()
        .timestamp
        .unwrap();
    assert!(minimum.physical < i64::MAX);
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
        &["--name=deprecated-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    scenarios::observe(
        &mut PdClient::connect(follower.url.clone()).await.unwrap(),
        id,
    )
    .await;
    let status = PdClient::connect(follower.url.clone())
        .await
        .unwrap()
        .get_min_ts(pdpb::GetMinTsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unavailable);
    follower.stop().unwrap();
    follower_node.shutdown().await.unwrap();
    println!(
        "deprecated RPC responses match Go on leader and follower with missing/wrong headers, extreme timestamp and unknown DC; HTTP ping and post-cancellation requests succeed"
    );
    go.stop().unwrap();
    node.shutdown().await.unwrap();
}
