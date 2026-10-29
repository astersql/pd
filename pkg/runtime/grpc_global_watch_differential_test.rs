// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::pd_client::PdClient;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_global_watch_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_global_watch_differential() {
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
                    cluster_id: 3302,
                    member_id: 1,
                    name: "watch".into(),
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
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = Server::start_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
        std::slice::from_ref(&go.url),
        std::time::Duration::from_secs(2),
    )
    .await
    .unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    assert_eq!(
        scenarios::observe(&mut rust, store.as_ref()).await,
        scenarios::observe(&mut client, &go_store).await
    );
    go.stop().unwrap();
    node.shutdown().await.unwrap();
}
