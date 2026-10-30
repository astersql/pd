// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_068_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 768,
                member_id: 1,
                name: "rule".into(),
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
        crate::http_member_test::request(&url, "POST", "/pd/api/v1/config/rule", "{}"),
        (
            500,
            serde_json::json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            )
        )
    );
    node.shutdown().await.unwrap();
}

use crate::http_member_test::request;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
fn normalized(mut v: Value) -> Value {
    match &mut v {
        Value::Object(o) => {
            o.remove("create_timestamp");
            for x in o.values_mut() {
                *x = normalized(x.take());
            }
        }
        Value::Array(a) => {
            for x in a {
                *x = normalized(x.take());
            }
        }
        _ => {}
    }
    v
}
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    for (method, path, body) in [
        ("GET", "rule/pd/default", ""),
        ("POST", "rule", "{"),
        ("GET", "rules/key/zz", ""),
    ] {
        out.push(json!([
            "cold",
            method,
            path,
            request(url, method, &format!("/pd/api/v1/config/{path}"), body)
        ]));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    // Explicitly align placement defaults, whose persisted enable flag differs before bootstrap.
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    for mode in ["table", "txn"] {
        assert_eq!(
            request(
                url,
                "POST",
                "/pd/api/v1/config",
                &json!({"pd-server.key-type":mode}).to_string()
            )
            .0,
            200
        );
        for body in [
            r#"{"group_id":"encoded","id":"test","role":"voter","count":1,"start_key":"61"}"#,
            r#"{"group_id":"encoded","id":"test","role":"voter","count":1,"end_key":"7a"}"#,
            r#"{"group_id":"encoded","id":"test","role":"voter","count":1,"start_key":"6100000000000000f8","end_key":"7a00000000000000f8"}"#,
        ] {
            let result = request(url, "POST", "/pd/api/v1/config/rule", body);
            out.push(json!([mode, body, normalized(result.1), result.0]));
        }
    }
    assert_eq!(
        request(url, "DELETE", "/pd/api/v1/config/rule/encoded/test", "").0,
        200
    );
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config",
            r#"{"pd-server.key-type":"raw"}"#
        )
        .0,
        200
    );
    for (method, path, body) in [
        ("GET", "rule/missing/id", ""),
        ("GET", "rules/group/missing", ""),
        (
            "POST",
            "rule",
            r#"{"group_id":"job+group","id":"A","start_key":"61","end_key":"6D","role":"voter","count":2}"#,
        ),
        ("GET", "rule/job+group/A", ""),
        ("GET", "rule/job%2Bgroup/A", ""),
        ("GET", "rules/group/job+group", ""),
        ("GET", "rules/key/62", ""),
        ("GET", "rules/key/6d", ""),
        ("GET", "rules/key/zz", ""),
        ("GET", "rules/key/a", ""),
        ("GET", "rules/region/10", ""),
        ("GET", "rules/region/11", ""),
        (
            "POST",
            "rule",
            r#"{"group_id":"cross","id":"split","start_key":"62","end_key":"63","role":"voter","count":1}"#,
        ),
        ("GET", "rules/region/10", ""),
        ("DELETE", "rule/cross/split", ""),
        ("GET", "rules/region/999", ""),
        ("GET", "rules/region/no", ""),
        (
            "POST",
            "rule",
            r#"{"group_id":"job+group","id":"A","start_key":"6D","end_key":"7a","role":"voter","count":1}"#,
        ),
        ("GET", "rules/key/62", ""),
        ("GET", "rules/key/6e", ""),
        (
            "POST",
            "rule",
            r#"{"group_id":"bad","id":"a","role":"voter","count":0}"#,
        ),
        (
            "POST",
            "rule",
            r#"{"group_id":"bad","id":"a","role":"unknown","count":1}"#,
        ),
        (
            "POST",
            "rule",
            r#"{"group_id":"bad","id":"a","role":"voter","count":1,"start_key":"zz"}"#,
        ),
        (
            "POST",
            "rule",
            r#"{"group_id":"bad","id":"a","role":"voter","count":1,"start_key":"7a","end_key":"61"}"#,
        ),
        ("POST", "rule", r#"{"count":"bad"}"#),
        ("POST", "rule", r#"{"count":1.5}"#),
        ("POST", "rule", r#"{"role":1}"#),
        ("POST", "rule", r#"{"location_labels":false}"#),
        ("POST", "rule", r#"{"group_id":3}"#),
        ("POST", "rule", r#"[]"#),
        ("POST", "rule", r#"42"#),
        ("POST", "rule", r#"null"#),
        ("POST", "rule", "{}"),
        ("POST", "rule", "{"),
        (
            "POST",
            "rule",
            r#"{"group_id":"pd","id":"default","role":"voter","count":4}"#,
        ),
        ("GET", "rule/pd/default", ""),
        (
            "POST",
            "rule",
            r#"{"group_id":"pd","id":"default","role":"bad","count":3}"#,
        ),
        ("GET", "rule/pd/default", ""),
        ("DELETE", "rule/job+group/A", ""),
        ("GET", "rule/job+group/A", ""),
        ("DELETE", "rule/job+group/A", ""),
        ("DELETE", "rule/pd/default", ""),
        ("GET", "rule/pd/default", ""),
    ] {
        let result = request(url, method, &format!("/pd/api/v1/config/{path}"), body);
        out.push(json!([method, path, normalized(result.1), result.0]));
    }
    let encoded = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "DELETE",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/config/rule/job%2Bgroup%2FA/extra"),
        ])
        .output()
        .unwrap();
    assert!(encoded.status.success());
    out.push(json!([
        "encoded-slash",
        String::from_utf8(encoded.stdout).unwrap()
    ]));
    out.push(json!([
        "replicas",
        request(url, "GET", "/pd/api/v1/config/replicate", "").1["max-replicas"]
    ]));
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"false"}"#
        )
        .0,
        200
    );
    out.push(json!([
        "disabled",
        request(url, "POST", "/pd/api/v1/config/rule", "{")
    ]));
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_068_queries_mutation_and_restart() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7682,
        member_id: 1,
        name: "rule".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let observations = observe(&url, 7682).await;
    println!("Rust observations: {observations}");
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/rule",
            r#"{"group_id":"persist","id":"one","role":"voter","count":2}"#
        )
        .0,
        200
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7682,
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(
        request(
            &follower_url,
            "GET",
            "/pd/api/v1/config/rule/persist/one",
            ""
        )
        .1["count"],
        2
    );
    assert_eq!(
        request(
            &follower_url,
            "POST",
            "/pd/api/v1/config/rule",
            r#"{"group_id":"redirect","id":"one","role":"voter","count":1}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/rule/redirect/one", "").1["count"],
        1
    );
    assert_eq!(
        request(
            &follower_url,
            "DELETE",
            "/pd/api/v1/config/rule/redirect/one",
            ""
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/rule/redirect/one", "").0,
        404
    );
    follower.shutdown().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let ids = node.cluster().pending_processed_regions().await.unwrap();
            if ids.contains(&10) && ids.contains(&11) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let encoded_slash = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "DELETE",
            "-w",
            "%{http_code}",
            &format!("{url}/pd/api/v1/config/rule/persist%2Fone/extra"),
        ])
        .output()
        .unwrap();
    assert!(encoded_slash.status.success());
    assert_eq!(
        String::from_utf8(encoded_slash.stdout).unwrap(),
        "404 page not found\n404"
    );
    let before = request(&url, "GET", "/pd/api/v1/config/rule/persist/one", "");
    assert_eq!(before.1["count"], 2);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7682);
    assert!(
        !endpoint
            .load_rule(&paths, "70657273697374-6f6e65")
            .await
            .unwrap()
            .is_empty()
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(
        normalized(request(&url, "GET", "/pd/api/v1/config/rule/persist/one", "").1),
        normalized(before.1)
    );
    node.shutdown().await.unwrap();
}
fn durable_setup(url: &str) {
    assert_eq!(request(url,"POST","/pd/api/v1/config/rule",r#"{"group_id":"durable","id":"kept","role":"voter","count":2,"start_key":"61","end_key":"6D","version":12,"create_timestamp":100}"#).0,200);
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/rule",
            r#"{"group_id":"durable","id":"gone","role":"voter","count":1}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(url, "DELETE", "/pd/api/v1/config/rule/durable/gone", "").0,
        200
    );
}
fn durable_view(url: &str) -> Value {
    json!([
        normalized(request(url, "GET", "/pd/api/v1/config/rule/durable/kept", "").1),
        request(url, "GET", "/pd/api/v1/config/rule/durable/gone", ""),
        normalized(request(url, "GET", "/pd/api/v1/config/rule/pd/default", "").1),
        request(url, "GET", "/pd/api/v1/config/replicate", "").1["max-replicas"]
    ])
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_068_differential() {
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
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
    let oracle = observe(&go.url, id).await;
    durable_setup(&go.url);
    let go_before = durable_view(&go.url);
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let go_after = durable_view(&go.url);
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7683,
        member_id: 1,
        name: "rule".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&url, 7683).await;
    durable_setup(&url);
    let rust_before = durable_view(&url);
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust_after = durable_view(&url);
    node.shutdown().await.unwrap();
    println!(
        "Go: {oracle}\nRust: {rust}\nGo durable before/after: {go_before}/{go_after}\nRust durable before/after: {rust_before}/{rust_after}"
    );
    assert_eq!(oracle, rust);
    assert_eq!(go_before, rust_before);
    assert_eq!(go_after, rust_after);
}
