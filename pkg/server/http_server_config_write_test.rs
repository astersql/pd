// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_050_behavior() {
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
        "/pd/api/v1/config",
        r#"{"pd-server.key-type":"raw"}"#,
    );
    assert_eq!(response, (200, serde_json::json!("The config is updated.")));
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/pd-server", "").1["key-type"],
        "raw"
    );
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config",
            &serde_json::json!({"dashboard-address":url}).to_string()
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/pd-server", "").1["dashboard-address"],
        url
    );
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config",
            r#"{"dashboard-address":"auto"}"#
        )
        .0,
        200
    );
    let before = request(&url, "GET", "/pd/api/v1/config/pd-server", "");
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config",
            r#"{"key-type":"invalid"}"#
        )
        .0,
        400
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/pd-server", ""),
        before
    );
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/cluster-version",
            r#"{"cluster-version":"v4.0.0"}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/cluster-version", "").1,
        "4.0.0"
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let persisted: serde_json::Value = endpoint
        .load_config(&astersql_pd_keypath::ClusterPaths::new(747))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted["pd-server"]["key-type"], "raw");
    assert_eq!(persisted["cluster-version"], "4.0.0");
    node.shutdown().await.unwrap();
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
        request(&url, "GET", "/pd/api/v1/config/pd-server", ""),
        before
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/cluster-version", "").1,
        "4.0.0"
    );
    node.shutdown().await.unwrap();
}

#[ignore = "requires existing Go binary and read-only supervisor"]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_050_differential() {
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
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

    // Align the intentionally different preexisting placement-rule defaults.
    for u in [&go.url, &url] {
        crate::http_replication_config_write_test::bootstrap(u).await;
        assert_eq!(
            request(
                u,
                "POST",
                "/pd/api/v1/config/replicate",
                r#"{"enable-placement-rules":"true"}"#
            )
            .0,
            200
        );
    }
    let mut differences = Vec::new();
    for (path, bodies) in [
        (
            "/pd/api/v1/config",
            vec![
                r#"{"pd-server.key-type":"raw"}"#,
                r#"{"key-type":"txn"}"#,
                r#"{"key-type":"bad"}"#,
                r#"{"flow-round-by-digit":7}"#,
                r#"{"flow-round-by-digit":8.0}"#,
                r#"{"flow-round-by-digit":-1}"#,
                r#"{"server-memory-limit":0.5}"#,
                r#"{"server-memory-limit":1}"#,
                r#"{"gc-keyspace-index-load-concurrency":0}"#,
                r#"{"gc-state-cache-warmup-concurrency":8}"#,
                r#"{"use-region-storage":"false"}"#,
                r#"{"use-region-storage":true}"#,
                r#"{"enable-gogc-tuner":"false"}"#,
                r#"{"block-safe-point-v1":"true"}"#,
                r#"{"runtime-services":"a,b"}"#,
                r#"{"runtime-services":null}"#,
                r#"{"key-type":null}"#,
                r#"{"key-type":42}"#,
                r#"{"flow-round-by-digit":"bad"}"#,
                r#"{"flow-round-by-digit":1.5}"#,
                r#"{"pd-server.unknown":1}"#,
                r#"{"unknown":1}"#,
                r#"{"name":"new"}"#,
                r#"{"pd-server":{}}"#,
                r#"{"label-property":{}}"#,
                r#"{"dashboard-address":"none"}"#,
                r#"{"dashboard-address":"http://127.0.0.1:9999"}"#,
                r#"{"metric-storage":"http://metric"}"#,
                r#"{"min-resolved-ts-persistence-interval":"20ms"}"#,
                r#"{"max-gap-reset-ts":"2h"}"#,
                r#"{"max-gap-reset-ts":null}"#,
                r#"{"max-gap-reset-ts":42}"#,
                r#"{"max-gap-reset-ts":"2"}"#,
                r#"{"max-gap-reset-ts":"bad"}"#,
                r#"{"max-gap-reset-ts":"2foo"}"#,
                r#"{"server-memory-limit-gc-trigger":0.8}"#,
                r#"{"server-memory-limit-gc-trigger":0.3}"#,
                r#"{"gc-tuner-threshold":0.7}"#,
                r#"{"gc-tuner-threshold":1}"#,
                r#"{"cluster-version":"v4.0.0"}"#,
                r#"{"cluster-version":"bad"}"#,
                r#"{"cluster-version":42}"#,
                r#"{"max-replicas":5}"#,
                r#"{"schedule.leader-schedule-limit":6}"#,
                r#"{"leader-schedule-limit":7,"schedule.leader-schedule-limit":8}"#,
                "{}",
                "null",
                "[]",
                "42",
                "true",
                "{",
            ],
        ),
        (
            "/pd/api/v1/config/cluster-version",
            vec![
                r#"{"cluster-version":"v5.1.0"}"#,
                r#"{"cluster-version":""}"#,
                r#"{"cluster-version":"bad"}"#,
                r#"{"cluster-version":null}"#,
                r#"{"cluster-version":"1.2.x"}"#,
                r#"{"cluster-version":"v"}"#,
                r#"{"cluster-version":"1.2.3-a!"}"#,
                r#"{"cluster-version":"1.2.3-rc.1+build"}"#,
                r#"{"cluster-version":42}"#,
                r#"{"other":42}"#,
                "{}",
                "null",
                "[]",
                "true",
                "42",
                "{",
            ],
        ),
    ] {
        for body in bodies {
            let expected = request(&go.url, "POST", path, body);
            let actual = request(&url, "POST", path, body);
            println!("{path} {body}: Go={expected:?} Rust={actual:?}");
            if normalize(actual.clone()) != normalize(expected.clone()) {
                differences.push(format!("{body}: {actual:?} != {expected:?}"));
            }
            for section in ["pd-server", "cluster-version"] {
                assert_eq!(
                    request(&url, "GET", &format!("/pd/api/v1/config/{section}"), ""),
                    request(&go.url, "GET", &format!("/pd/api/v1/config/{section}"), ""),
                    "state after {body}"
                );
            }
        }
    }
    node.shutdown().await.unwrap();
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
fn normalize(mut r: (u16, serde_json::Value)) -> (u16, serde_json::Value) {
    if let Some(v) = r.1.get_mut("stack") {
        assert!(v.is_array());
        *v = serde_json::json!([]);
    }
    r
}
