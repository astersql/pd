// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::pd_client::PdClient;
use astersql_pd_storage::{MetaStore, Mutation};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
async fn seed(store: &dyn MetaStore, controller: &[u8]) {
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"resource_group/controller", controller),
                Mutation::put(b"resource_group/controller-other", b"sibling-value"),
            ],
        )
        .await
        .unwrap();
}

#[path = "../server/grpc_global_config_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_global_config_differential() {
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
                    cluster_id: 3202,
                    member_id: 1,
                    name: "global".into(),
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
    // Match the isolated backend's request limit, rather than Go PD's 150 MiB default.
    let config = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_WORK_DIR").unwrap())
        .join("global-config.toml");
    std::fs::write(&config, "max-request-bytes = 1572864\n").unwrap();
    let config_arg = format!("--config={}", config.display());
    let mut leader = Server::start_with_args(&binary, &[&config_arg]).unwrap();
    leader.ready().unwrap();
    let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
        std::slice::from_ref(&leader.url),
        std::time::Duration::from_secs(2),
    )
    .await
    .unwrap();
    // Use the existing oracle's initialized bytes as identical fixture input.
    // A joining Go member can re-save this same controller configuration.
    let controller = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let snapshot = go_store
                .read(astersql_pd_storage::Read::range(
                    b"resource_group/controller",
                    b"",
                    astersql_pd_storage::Consistency::Linearizable,
                ))
                .await
                .unwrap();
            if let Some(entry) = snapshot.entries.into_iter().next() {
                break entry.value;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    println!(
        "controller fixture from oracle: {}",
        String::from_utf8_lossy(&controller)
    );
    seed(store.as_ref(), &controller).await;
    seed(&go_store, &controller).await;
    let expected = scenarios::observe(&mut rust).await;
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    assert_eq!(expected, scenarios::observe(&mut client).await);
    println!(
        "global config serialized responses, invalid paths and persisted reads match Go before bootstrap"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                crate::NodeConfig {
                    cluster_id: 3202,
                    member_id: 2,
                    name: "global-follower".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    let mut rust = PdClient::connect(url).await.unwrap();
    let join = format!("--join={}", leader.url);
    let mut go_follower = Server::start_with_args(
        &binary,
        &[
            "--name=global-follower",
            "--initial-cluster=",
            &join,
            &config_arg,
        ],
    )
    .unwrap();
    go_follower.ready().unwrap();
    seed(store.as_ref(), &controller).await;
    seed(&go_store, &controller).await;
    let expected = scenarios::observe(&mut rust).await;
    let mut client = PdClient::connect(go_follower.url.clone()).await.unwrap();
    assert_eq!(expected, scenarios::observe(&mut client).await);
    println!("global config shared writes/reads match Go on follower");
    go_follower.stop().unwrap();
    leader.stop().unwrap();
    std::fs::remove_file(config).unwrap();
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
}
