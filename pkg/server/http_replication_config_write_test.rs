// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_049_behavior() {
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
        "/pd/api/v1/config/replicate",
        r#"{"max-replicas":5}"#,
    );
    assert_eq!(response, (200, serde_json::json!("The config is updated.")));
    let read = || request(&url, "GET", "/pd/api/v1/config/replicate", "");
    assert_eq!(read().1["max-replicas"], 5);
    let before = read();
    let invalid = request(
        &url,
        "POST",
        "/pd/api/v1/config/replicate",
        r#"{"max-replicas":101,"isolation-level":"bad"}"#,
    );
    assert_eq!(
        invalid,
        (
            500,
            serde_json::json!("isolation-level must be one of location-labels or empty")
        )
    );
    assert_eq!(read(), before);
    for body in [
        r#"{"action":"set","type":"reject-leader","label-key":"zone","label-value":"z1"}"#,
        r#"{"action":"set","type":"reject-leader","label-key":"zone","label-value":"z1"}"#,
    ] {
        assert_eq!(
            request(&url, "POST", "/pd/api/v1/config/label-property", body).0,
            200
        );
    }
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/label-property", "").1,
        serde_json::json!({"reject-leader":[{"key":"zone","value":"z1"}]})
    );
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/label-property",
            r#"{"action":"bad"}"#
        ),
        (500, serde_json::json!("unknown action bad"))
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let persisted: serde_json::Value = endpoint
        .load_config(&astersql_pd_keypath::ClusterPaths::new(747))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted["replication"]["max-replicas"], 5);
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
        request(&url, "GET", "/pd/api/v1/config/replicate", "").1["max-replicas"],
        5
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/label-property", "").1,
        serde_json::json!({"reject-leader":[{"key":"zone","value":"z1"}]})
    );
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/label-property",
            r#"{"action":"delete","type":"reject-leader","label-key":"zone","label-value":"z1"}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/label-property", "").1,
        serde_json::json!({})
    );
    node.shutdown().await.unwrap();
}

pub(crate) async fn bootstrap(url: &str) {
    use astersql_pd_proto::{metapb, pdpb};
    let mut client = pdpb::pd_client::PdClient::connect(url.to_owned())
        .await
        .unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let result = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_049_differential() {
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
                cluster_id: 749,
                member_id: 1,
                name: "replicate".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    for u in [&go.url, &url] {
        assert_eq!(
            request(
                u,
                "POST",
                "/pd/api/v1/config/replicate",
                r#"{"enable-placement-rules":"true"}"#
            ),
            (
                500,
                serde_json::json!(
                    "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
                )
            )
        );
    }
    for u in [&go.url, &url] {
        bootstrap(u).await;
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
    for body in [
        r#"{"max-replicas":5}"#,
        r#"{"location-labels":"zone,rack","isolation-level":"zone"}"#,
        r#"{"max-replicas":6,"isolation-level":"bad"}"#,
        r#"{"max-replicas":0}"#,
        r#"{"max-replicas":"bad"}"#,
        r#"{"max-replicas":-1}"#,
        r#"{"isolation-level":42}"#,
        r#"{"max-replicas":{}}"#,
        r#"{"max-replicas":[]}"#,
        r#"{"max-replicas":1.5}"#,
        r#"{"max-repli\u0063as":"bad"}"#,
        r#"{"location-labels":null}"#,
        r#"{"location-labels":["zone"]}"#,
        r#"{"enable-placement-rules":true}"#,
        r#"{"enable-placement-rules":"bad"}"#,
        r#"{"max-replicas":null}"#,
        r#"{"isolation-level":null}"#,
        r#"{"strictly-match-label":"true"}"#,
        r#"{"unknown":42}"#,
        "{}",
        "null",
        "[]",
        "true",
        "42",
        r#""bad""#,
        "{",
        r#"{"enable-placement-rules":"false"}"#,
        r#"{"max-replicas":7}"#,
        r#"{"enable-placement-rules":"true","max-replicas":8}"#,
        r#"{"enable-placement-rules":"true"}"#,
    ] {
        if body == "null" {
            let expected = raw_null(&go.url);
            let actual = raw_null(&url);
            println!("null Go={expected:?}, Rust={actual:?}");
            // Recovery stacks contain Go frame names, addresses and goroutine IDs.
            // Compare the failure heading, HTTP status/type and unchanged config.
            let parts = |s: String| {
                let (body, metadata) = s.rsplit_once("\n500\n").unwrap();
                (body.lines().next().unwrap().to_owned(), metadata.to_owned())
            };
            assert_eq!(parts(actual), parts(expected));
            assert_eq!(
                request(&url, "GET", "/pd/api/v1/config/replicate", ""),
                request(&go.url, "GET", "/pd/api/v1/config/replicate", "")
            );
            continue;
        }
        let expected = request(&go.url, "POST", "/pd/api/v1/config/replicate", body);
        let actual = request(&url, "POST", "/pd/api/v1/config/replicate", body);
        assert_eq!(
            normalize(actual.clone()),
            normalize(expected),
            "body={body}"
        );
        assert_eq!(
            request(&url, "GET", "/pd/api/v1/config/replicate", ""),
            request(&go.url, "GET", "/pd/api/v1/config/replicate", ""),
            "read after {body}"
        );
        println!("replication {body}: {actual:?}");
    }
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store);
    let rule: serde_json::Value = serde_json::from_slice(
        &endpoint
            .load_rule(
                &astersql_pd_keypath::ClusterPaths::new(749),
                "7064-64656661756c74",
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let go_rule = request(&go.url, "GET", "/pd/api/v1/config/rule/pd/default", "");
    assert_eq!(go_rule.0, 200);
    let semantic_rule = |mut value: serde_json::Value| {
        let fields = value.as_object_mut().unwrap();
        fields.remove("version");
        fields.remove("create_timestamp");
        value
    };
    assert_eq!(semantic_rule(rule.clone()), semantic_rule(go_rule.1));
    assert_eq!(rule["count"], 5);
    assert_eq!(rule["location_labels"], serde_json::json!(["zone", "rack"]));
    for body in [
        r#"{"action":"set","type":"reject-leader","label-key":"zone","label-value":"z1"}"#,
        r#"{"action":"set","type":"reject-leader","label-key":"zone","label-value":"z1"}"#,
        r#"{"action":"delete","type":"absent"}"#,
        r#"{"action":"delete","type":"reject-leader","label-key":"zone","label-value":"z1"}"#,
        r#"{"action":"set"}"#,
        r#"{"action":"unknown"}"#,
        r#"{"action":3}"#,
        "null",
        "{}",
        "[]",
        "true",
        "42",
        r#""bad""#,
        "{",
    ] {
        let expected = request(&go.url, "POST", "/pd/api/v1/config/label-property", body);
        let actual = request(&url, "POST", "/pd/api/v1/config/label-property", body);
        assert_eq!(actual, expected, "label body={body}");
        assert_eq!(
            request(&url, "GET", "/pd/api/v1/config/label-property", ""),
            request(&go.url, "GET", "/pd/api/v1/config/label-property", "")
        );
        println!("labels {body}: {actual:?}");
    }
    node.shutdown().await.unwrap();
}

// Go error stack entries are process-specific program counters; retain the field
// and every other error property, normalizing only the addresses.
fn normalize(mut response: (u16, serde_json::Value)) -> (u16, serde_json::Value) {
    if let Some(stack) = response.1.get_mut("stack") {
        assert!(stack.is_array());
        *stack = serde_json::json!([]);
    }
    response
}

fn raw_null(url: &str) -> String {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "POST",
            "--data-binary",
            "null",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/config/replicate"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
