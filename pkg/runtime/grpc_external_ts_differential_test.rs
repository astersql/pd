// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_external_ts_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_external_timestamp_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = crate::NodeConfig {
        cluster_id: 3501,
        member_id: 1,
        name: "external-diff".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                config.clone(),
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let mut rust = PdClient::connect(url.clone()).await.unwrap();
    let admission = scenarios::admission(&mut rust, 3501).await;
    prepare::prepare(&mut rust, 3501).await;
    let expected = scenarios::observe(&mut rust, 3501).await;
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    assert_eq!(
        storage
            .load_external_ts(&astersql_pd_keypath::ClusterPaths::new(3501))
            .await
            .unwrap(),
        102
    );
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
    assert_eq!(admission, scenarios::admission(&mut client, id).await);
    prepare::prepare(&mut client, id).await;
    assert_eq!(expected, scenarios::observe(&mut client, id).await);
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{}/pd/api/v1/ping", go.url)])
            .output()
            .unwrap()
            .status
            .success()
    );
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    assert_eq!(scenarios::get(&mut client, id).await.timestamp, 102);
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                config,
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let mut rust = PdClient::connect(url.clone()).await.unwrap();
    assert_eq!(scenarios::get(&mut rust, 3501).await.timestamp, 102);
    let ping = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
        .output()
        .unwrap();
    assert!(ping.status.success());
    println!(
        "External timestamps: full normalized protocol responses, exact errors, monotonic updates, HTTP ping and restart persistence match Go"
    );
    go.stop().unwrap();
    node.shutdown().await.unwrap();
}
