// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_069_behavior() {
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
        crate::http_member_test::request(&url, "POST", "/pd/api/v1/config/rules/batch", "{}"),
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
fn normalized(mut value: Value) -> Value {
    match &mut value {
        Value::Object(o) => {
            o.remove("create_timestamp");
            for v in o.values_mut() {
                *v = normalized(v.take());
            }
        }
        Value::Array(a) => {
            for v in a {
                *v = normalized(v.take());
            }
        }
        _ => {}
    }
    value
}
fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    request(
        url,
        method,
        &format!("/pd/api/v1/config/{path}").trim_end_matches('/'),
        body,
    )
}
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    for path in [
        "rules",
        "rules/batch",
        "rule_groups",
        "rule_group",
        "placement-rule",
    ] {
        let (method, body) = if path == "rules/batch" || path == "rule_group" {
            ("POST", "{")
        } else {
            ("GET", "")
        };
        let result = call(url, method, path, body);
        assert_eq!(result.0, 500);
        out.push(json!(["cold", path, result]));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    // A single peer exercises both nil orphan lists and empty rule-fit peer lists.
    let peer = astersql_pd_proto::metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        }),
        region: Some(astersql_pd_proto::metapb::Region {
            id: 13,
            start_key: b"z".to_vec(),
            end_key: b"zz".to_vec(),
            peers: vec![peer.clone()],
            region_epoch: Some(astersql_pd_proto::metapb::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let _stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let region = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: id,
                        ..Default::default()
                    }),
                    region_id: 13,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if region.region.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    assert_eq!(
        call(
            url,
            "POST",
            "replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    assert_eq!(
        call(url, "POST", "", r#"{"pd-server.key-type":"raw"}"#).0,
        200
    );
    // Every successful or rejected mutation is followed by all-rules/groups snapshots.
    let cases = [
        ("GET", "rules", "", 200),
        ("GET", "rule_groups", "", 200),
        ("GET", "rule_group/missing", "", 404),
        ("GET", "placement-rule/missing", "", 200),
        ("GET", "rules/region/no/detail", "", 400),
        ("GET", "rules/region/999/detail", "", 404),
        ("GET", "rules/region/10/detail", "", 200),
        ("GET", "rules/region/13/detail", "", 200),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"z","id":"p1","role":"voter","count":1,"action":"add"},{"group_id":"z","id":"p2","role":"voter","count":2,"action":"add"}]"#,
            200,
        ),
        ("GET", "rules/region/13/detail", "", 200),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"no","id":"first","role":"voter","count":1,"action":"add"},{"group_id":"no","id":"bad","role":"voter","count":0,"action":"add"}]"#,
            400,
        ),
        ("GET", "rule/no/first", "", 404),
        (
            "POST",
            "rule_group",
            r#"{"id":"z","index":-2,"override":true}"#,
            200,
        ),
        ("POST", "rule_group", r#"{"id":"a","index":-3}"#, 200),
        ("GET", "rules/key/61", "", 200),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"z","id":"p","action":"del","delete_by_id_prefix":true},{"group_id":"z","id":"p3","role":"voter","count":1,"action":"add"}]"#,
            200,
        ),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"z","id":"ignored","action":"unknown"}]"#,
            200,
        ),
        ("DELETE", "rule_group/z", "", 200),
        ("GET", "rule/z/p3", "", 200),
        (
            "POST",
            "placement-rule/g",
            r#"{"group_index":2,"group_override":true,"rules":[{"id":"one","role":"voter","count":1}]}"#,
            200,
        ),
        ("GET", "placement-rule/g", "", 200),
        (
            "POST",
            "placement-rule/g",
            r#"{"group_id":"wrong","rules":[]}"#,
            400,
        ),
        (
            "POST",
            "placement-rule/g",
            r#"{"rules":[{"group_id":"wrong","id":"bad","role":"voter","count":1}]}"#,
            400,
        ),
        (
            "POST",
            "placement-rule?partial=false",
            r#"[{"group_id":"h","group_index":1,"rules":[{"id":"one","role":"voter","count":1}]}]"#,
            200,
        ),
        ("GET", "placement-rule/g", "", 200),
        (
            "POST",
            "placement-rule?partial",
            r#"[{"group_id":"g","rules":[{"id":"two","role":"voter","count":2}]}]"#,
            200,
        ),
        ("GET", "rule/g/one", "", 404),
        (
            "POST",
            "placement-rule",
            r#"[{"group_id":"broken","rules":[{"id":"bad","role":"voter","count":0}]}]"#,
            400,
        ),
        (
            "POST",
            "rules",
            r#"[{"group_id":"upsert","id":"one","role":"voter","count":1}]"#,
            200,
        ),
        (
            "POST",
            "rules",
            r#"[{"group_id":"absent","id":"one","role":"voter","count":1},{"group_id":"absent","id":"bad","role":"learner","count":0}]"#,
            400,
        ),
        (
            "POST",
            "rules",
            r#"[{"group_id":"pd","id":"default","role":"voter","count":2},{"group_id":"invalid","id":"one","role":"voter","count":0}]"#,
            400,
        ),
        ("GET", "replicate", "", 200),
        (
            "POST",
            "rules",
            r#"[{"group_id":"pd","id":"default","role":"voter","count":3}]"#,
            200,
        ),
        ("GET", "replicate", "", 200),
        ("POST", "rules/batch", "null", 200),
        ("POST", "rules", "[]", 200),
        (
            "DELETE",
            "placement-rule/%5E%28g%7Ch%29%24?regexp=false",
            "",
            200,
        ),
        ("GET", "placement-rule/g", "", 200),
        (
            "POST",
            "placement-rule",
            r#"[{"group_id":"final","group_index":4,"rules":[{"id":"one","role":"voter","count":3}]}]"#,
            200,
        ),
        ("DELETE", "placement-rule/final", "", 500),
        ("POST", "placement-rule", r#"[]"#, 500),
        ("POST", "rule_group", r#"{"id":"final","index":-1}"#, 200),
        ("GET", "rules/region/10/detail", "", 200),
        ("GET", "rules/region/13/detail", "", 200),
        ("POST", "rules/batch", r#"{}"#, 400),
        ("POST", "rules/batch", r#"[{"action":42}]"#, 400),
        ("POST", "rules/batch", r#"[{"count":"bad"}]"#, 400),
        ("POST", "rules/batch", r#"[{"count":1.5}]"#, 400),
        (
            "POST",
            "rules/batch",
            r#"[{"delete_by_id_prefix":"false"}]"#,
            400,
        ),
        ("POST", "rules", r#"[{"location_labels":[1]}]"#, 400),
        ("POST", "rule_group", r#"[]"#, 400),
        ("POST", "rule_group", r#"{"index":"bad"}"#, 400),
        ("POST", "rule_group", r#"{"id":3}"#, 400),
        (
            "POST",
            "placement-rule/final",
            r#"{"rules":[{"count":"bad"}]}"#,
            400,
        ),
        ("POST", "placement-rule/final", r#"{"rules":3}"#, 400),
        (
            "POST",
            "placement-rule",
            r#"[{"rules":[{"role":42}]}]"#,
            400,
        ),
        ("POST", "placement-rule", r#"[{"group_index":false}]"#, 400),
        (
            "POST",
            "rules/batch",
            r#"[{"GROUP_ID":"case","ID":"one","ROLE":"voter","COUNT":1,"ACTION":"add"}]"#,
            200,
        ),
        ("GET", "rule/case/one", "", 200),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"case","id":"two","role":"voter","count":1,"action":"add"},{"group_id":"case","id":"","action":"del","delete_by_id_prefix":true}]"#,
            200,
        ),
        ("GET", "rule/case/two", "", 200),
        ("GET", "rule/case/one", "", 404),
        ("DELETE", "placement-rule/case", "", 200),
        ("DELETE", "placement-rule/%5B?regexp", "", 500),
        ("DELETE", "placement-rule/%28?regexp", "", 500),
        ("DELETE", "placement-rule/%5C%5C?regexp", "", 200),
        (
            "POST",
            "rules/batch",
            r#"[{"group_id":"dup","id":"one","count":1,"count":null,"role":"voter","action":"add","action":null}]"#,
            200,
        ),
        ("GET", "rule/dup/one", "", 200),
        (
            "POST",
            "rule_group",
            r#"{"id":"dup","index":5,"index":null}"#,
            200,
        ),
        (
            "POST",
            "rules/batch",
            r#"[{"count":"bad","count":null}]"#,
            400,
        ),
        (
            "POST",
            "rules/batch",
            r#"[{"label_constraints":[{"op":1}]}]"#,
            400,
        ),
        ("DELETE", "placement-rule/dup", "", 200),
        ("DELETE", "placement-rule/%25", "", 400),
        (
            "POST",
            "placement-rule/a%2Bb",
            r#"{"group_index":-7,"rules":[{"id":"one","role":"voter","count":1}]}"#,
            200,
        ),
        ("GET", "placement-rule/a+b", "", 200),
        ("GET", "rule_group/a%2Bb", "", 200),
        ("DELETE", "placement-rule/a+b", "", 200),
        ("POST", "rules/batch", "{", 400),
        ("POST", "rule_group", "{", 400),
        ("POST", "placement-rule", "{", 400),
    ];
    for (method, path, body, status) in cases {
        let result = call(url, method, path, body);
        assert_eq!(result.0, status, "{method} {path}: {result:?}");
        out.push(json!([method, path, body, result.0, normalized(result.1)]));
        out.push(json!([
            "snapshot",
            normalized(call(url, "GET", "rules", "").1),
            normalized(call(url, "GET", "placement-rule", "").1),
            call(url, "GET", "rule_groups", "").1
        ]));
    }
    // Feature gate must precede parsing, including the newly registered routes.
    assert_eq!(
        call(
            url,
            "POST",
            "replicate",
            r#"{"enable-placement-rules":"false"}"#
        )
        .0,
        200
    );
    out.push(json!([
        "disabled",
        call(url, "POST", "rules/batch", "{"),
        call(url, "GET", "rule_groups", "")
    ]));
    assert_eq!(
        call(
            url,
            "POST",
            "replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_069_bulk_atomic_and_restart() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    // Keep a duplicate listener FD bound through shutdown, preventing another
    // parallel fixture from claiming this member's immutable network identity.
    let bound_listener = listener.into_std().unwrap();
    let restart_listener = bound_listener.try_clone().unwrap();
    let listener = tokio::net::TcpListener::from_std(bound_listener).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7692,
        member_id: 1,
        name: "groups".into(),
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
    println!("Rust observations: {}", observe(&url, 7692).await);
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
    let before = normalized(call(&url, "GET", "placement-rule", "").1);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7692);
    let too_many=(0..150).map(|i|json!({"group_id":"overflow","id":format!("r{i}"),"role":"voter","count":1,"action":"add"})).collect::<Vec<_>>();
    let result = call(&url, "POST", "rules/batch", &json!(too_many).to_string());
    assert_eq!(
        result.0, 500,
        "backend capacity must reject the whole atomic transaction: {result:?}"
    );
    assert_eq!(
        normalized(call(&url, "GET", "placement-rule", "").1),
        before
    );
    let mut overflow = vec![];
    endpoint
        .load_rules(&paths, |_, v| {
            let value: Value = serde_json::from_slice(v).unwrap();
            if value["group_id"] == "overflow" {
                overflow.push(value);
            }
        })
        .await
        .unwrap();
    assert!(overflow.is_empty());
    let mut groups = vec![];
    endpoint
        .load_rule_groups(&paths, |k, v| groups.push((k.to_owned(), v.to_vec())))
        .await
        .unwrap();
    assert!(
        groups
            .iter()
            .any(|(_, v)| serde_json::from_slice::<Value>(v).unwrap()["index"] == -1)
    );
    // Follower must use leader state and the same serialized mutation queue.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    ..config.clone()
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(
        normalized(call(&follower_url, "GET", "placement-rule", "").1),
        before
    );
    assert_eq!(
        call(
            &follower_url,
            "POST",
            "rule_group",
            r#"{"id":"final","index":-1}"#
        )
        .0,
        200
    );
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::from_std(restart_listener).unwrap();
    let restart_url = url.clone();
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
        normalized(call(&restart_url, "GET", "placement-rule", "").1),
        before
    );
    node.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_069_differential() {
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
    let go_before = normalized(call(&go.url, "GET", "placement-rule", "").1);
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let go_after = normalized(call(&go.url, "GET", "placement-rule", "").1);
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    // Keep a duplicate listener FD bound through shutdown, preventing another
    // parallel fixture from claiming this member's immutable network identity.
    let bound_listener = listener.into_std().unwrap();
    let restart_listener = bound_listener.try_clone().unwrap();
    let listener = tokio::net::TcpListener::from_std(bound_listener).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7693,
        member_id: 1,
        name: "groups".into(),
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
    let rust = observe(&url, 7693).await;
    let rust_before = normalized(call(&url, "GET", "placement-rule", "").1);
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::from_std(restart_listener).unwrap();
    let restart_url = url.clone();
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
    let rust_after = normalized(call(&restart_url, "GET", "placement-rule", "").1);
    node.shutdown().await.unwrap();
    println!(
        "Go: {oracle}\nRust: {rust}\nGo restart: {go_before}/{go_after}\nRust restart: {rust_before}/{rust_after}"
    );
    assert_eq!(oracle, rust);
    assert_eq!(go_before, rust_before);
    assert_eq!(go_after, rust_after);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_069_storage_failure_and_cancel() {
    use crate::http_member_test::request;
    use crate::http_rule_fault_test::RuleStore;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(RuleStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7694,
                member_id: 1,
                name: "rule-fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 7694).await;
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    let body = r#"[{"group_id":"fault","id":"one","role":"voter","count":2,"action":"add"},{"group_id":"fault","id":"two","role":"voter","count":1,"action":"add"}]"#;
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        request(&url, "POST", "/pd/api/v1/config/rules/batch", body).0,
        500
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/rule/fault/one", "").0,
        404
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(inner.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7694);
    assert!(
        endpoint
            .load_rule(&paths, "6661756c74-6f6e65")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config/rule/fault/two", "").0,
        404
    );
    assert!(
        endpoint
            .load_rule(&paths, "6661756c74-74776f")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        request(&url, "POST", "/pd/api/v1/config/rules/batch", body).0,
        200
    );
    let group_before = call(&url, "GET", "rule_group/fault", "");
    let persisted_before = endpoint
        .load_rule(&paths, "6661756c74-6f6e65")
        .await
        .unwrap();
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(call(&url,"POST","placement-rule/fault",r#"{"group_index":-8,"rules":[{"id":"one","role":"voter","count":1},{"id":"two","role":"voter","count":2}]}"#).0,500);
    assert_eq!(call(&url, "GET", "rule_group/fault", ""), group_before);
    assert_eq!(
        endpoint
            .load_rule(&paths, "6661756c74-6f6e65")
            .await
            .unwrap(),
        persisted_before
    );
    let mut saved_groups = vec![];
    endpoint
        .load_rule_groups(&paths, |_, v| {
            saved_groups.push(serde_json::from_slice::<Value>(v).unwrap())
        })
        .await
        .unwrap();
    assert!(saved_groups.iter().all(|g| g["id"] != "fault"));
    let before = request(&url, "GET", "/pd/api/v1/config/rule/fault/one", "");
    let persisted = endpoint
        .load_rule(&paths, "6661756c74-6f6e65")
        .await
        .unwrap();
    let persisted_two = endpoint
        .load_rule(&paths, "6661756c74-74776f")
        .await
        .unwrap();
    assert_eq!(call(&url, "GET", "rule/fault/two", "").1["count"], 1);
    store.mode.store(2, Ordering::SeqCst);
    let target = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "10",
                "-X",
                "POST",
                "--data-binary",
                r#"[{"group_id":"fault","id":"one","role":"voter","count":1,"action":"add"},{"group_id":"fault","id":"two","role":"voter","count":2,"action":"add"}]"#,
                "-w",
                "%{http_code}",
                &format!("{target}/pd/api/v1/config/rules/batch"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let result = pending.await.unwrap();
    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout).unwrap().ends_with("500"));
    node.shutdown().await.unwrap();
    assert_eq!(
        endpoint
            .load_rule(&paths, "6661756c74-6f6e65")
            .await
            .unwrap(),
        persisted
    );
    assert_eq!(
        endpoint
            .load_rule(&paths, "6661756c74-74776f")
            .await
            .unwrap(),
        persisted_two
    );
    assert_eq!(before.1["count"], 2);
}
