// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_048_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 747,
                member_id: 1,
                name: "config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = request(
        &url,
        "POST",
        "/pd/api/v1/config/schedule",
        r#"{"max-snapshot-count":99}"#,
    );
    assert_eq!(response, (200, serde_json::json!("The config is updated.")));
    let read = || request(&url, "GET", "/pd/api/v1/config/schedule", "");
    assert_eq!(read().1["max-snapshot-count"], 99);
    let before = read();
    let invalid = request(
        &url,
        "POST",
        "/pd/api/v1/config/schedule",
        r#"{"max-snapshot-count":101,"low-space-ratio":-1}"#,
    );
    assert_eq!(
        invalid,
        (
            500,
            serde_json::json!("low-space-ratio should between 0 and 1")
        )
    );
    assert_eq!(read(), before);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let persisted: serde_json::Value = endpoint
        .load_config(&astersql_pd_keypath::ClusterPaths::new(747))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted["schedule"]["max-snapshot-count"], 99);
    node.shutdown().await.unwrap();
    assert_eq!(
        response.0, 200,
        "missing configuration read route: {response:?}"
    );
    assert_eq!(response.1, "The config is updated.");
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 747,
                member_id: 1,
                name: "config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/schedule", "").1["max-snapshot-count"],
        99
    );
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_048_differential() {
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 748,
                member_id: 1,
                name: "write".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    for body in [
        r#"{"region-score-formula-version":"v2"}"#,
        r#"{"max-snapshot-count":99}"#,
        r#"{"max-snapshot-count":101,"low-space-ratio":-1}"#,
        r#"{"halt-scheduling":"true"}"#,
        "null",
        "{}",
        r#"{"unknown":42}"#,
        r#"{"disable-raft-learner":"true"}"#,
        r#"{"max-snapshot-count":"bad"}"#,
        "{",
        "[]",
        r#"{"default-store-limit":{"add-peer":42},"store-limit":{"8":{"remove-peer":55}}}"#,
        r#"{"store-limit":{"8":{"add-peer":null},"9":{}}}"#,
        r#"{"store-limit":null}"#,
        r#"{"halt-scheduling":"false"}"#,
        r#"{"max-snapshot-count":-1}"#,
        r#"{"halt-scheduling":true}"#,
        r#"{"halt-scheduling":"bad"}"#,
        r#"{"split-merge-interval":"bad"}"#,
        r#"{"default-store-limit":[1,2,3]}"#,
        r#"{"store-limit":{"8":[1,2,3]}}"#,
        r#"{"default-store-li\u006dit":[1,2,3]}"#,
        r#"{"max-snapshot-co\u0075nt":"bad"}"#,
    ] {
        let expected = request(&go.url, "POST", "/pd/api/v1/config/schedule", body);
        let actual = request(&url, "POST", "/pd/api/v1/config/schedule", body);
        assert_eq!(actual, expected, "body={body}");
        let expected = request(&go.url, "GET", "/pd/api/v1/config/schedule", "");
        let actual = request(&url, "GET", "/pd/api/v1/config/schedule", "");
        assert_eq!(actual, expected, "configuration after {body}");
    }
    node.shutdown().await.unwrap();
}
