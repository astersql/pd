// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn observe(url: &str, cluster_id: u64) -> (Value, u64) {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let mut replies = Vec::new();
    let mut last = 0;
    for count in [0, 1, 5, 1005, 2] {
        let r = client
            .alloc_id(pdpb::AllocIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id,
                    ..Default::default()
                }),
                count,
            })
            .await
            .unwrap()
            .into_inner();
        let h = r.header.unwrap();
        assert_eq!(h.cluster_id, cluster_id);
        assert!(h.error.is_none());
        assert_eq!(r.count, if count > 1 { count } else { 0 });
        assert!(r.id - last >= u64::from(count.max(1)));
        println!(
            "AllocID count={count}, last={}, returned_count={}",
            r.id, r.count
        );
        replies.push(json!([count.max(1), r.count]));
        last = r.id;
    }
    assert_eq!(
        client
            .alloc_id(pdpb::AllocIdRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    let ping = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            &format!("{url}/pd/api/v1/ping"),
        ])
        .output()
        .unwrap();
    assert!(ping.status.success());
    assert_eq!(ping.stdout, b"200");
    (json!(replies), last)
}
async fn after_restart(url: &str, cluster_id: u64, last: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let r = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id,
                ..Default::default()
            }),
            count: 3,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    assert_eq!(r.count, 3);
    assert!(r.id - 2 > last);
    println!("AllocID restart last={}, previous={last}", r.id);
    json!([r.id % 1000, r.count, r.id - 2 > last])
}
async fn exhausted(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let r = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            count: 1001,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!((r.id, r.count), (0, 0));
    let e = r.header.unwrap().error.unwrap();
    assert_eq!(e.r#type, pdpb::ErrorType::Unknown as i32);
    assert_eq!(e.message, "[PD:idalloc:ErrIDExhausted]id exhausted");
    json!([e.r#type, e.message, r.id, r.count])
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_alloc_id_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = crate::NodeConfig {
        cluster_id: 2100,
        member_id: 1,
        name: "alloc-id".into(),
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
    let (rust, last) = observe(&url, 2100).await;
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
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
    let rust_restart = after_restart(&url, 2100, last).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut server = Server::start(&binary).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let (go, last) = observe(&server.url, id).await;
    server.restart_with_args(&binary, &[]).unwrap();
    server.ready().unwrap();
    let go_restart = after_restart(&server.url, id, last).await;
    println!(
        "Go AllocID: {go}, restart {go_restart}; Rust AllocID: {rust}, restart {rust_restart}"
    );
    assert_eq!(go, rust);
    assert_eq!(go_restart, rust_restart);
    use astersql_pd_storage::{MetaStore, Mutation};
    // Seed an aligned final window through the real etcd API, then restart both.
    let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
        &[server.url.clone()],
        std::time::Duration::from_secs(3),
    )
    .await
    .unwrap();
    go_store
        .transact(
            vec![],
            vec![Mutation::put(
                format!("/pd/{id}/alloc_id").as_bytes(),
                &(u64::MAX - 1000).to_be_bytes(),
            )],
        )
        .await
        .unwrap();
    server.restart_with_args(&binary, &[]).unwrap();
    server.ready().unwrap();
    let go_error = exhausted(&server.url, id).await;
    node.shutdown().await.unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/2100/alloc_id",
                &(u64::MAX - 1000).to_be_bytes(),
            )],
        )
        .await
        .unwrap();
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
    let rust_error = exhausted(&url, 2100).await;
    println!("Go exhaustion: {go_error}; Rust exhaustion: {rust_error}");
    assert_eq!(go_error, rust_error);
    server.stop().unwrap();
    node.shutdown().await.unwrap();
}
