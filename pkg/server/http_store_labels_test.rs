// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_053_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 752,
                member_id: 1,
                name: "state".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = request(&url, "POST", "/pd/api/v1/store/1/label", "{}");
    node.shutdown().await.unwrap();
    assert_eq!(
        response.0, 500,
        "unbootstrapped store mutation: {response:?}"
    );
}

async fn observe(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let mut output = vec![];
    let r = request(url, "POST", "/pd/api/v1/store/1/label", "{}");
    assert_eq!(r.0, 500);
    output.push(json!(r));
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1".into(),
                node_state: 1,
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
    assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");

    assert_eq!(request(url, "GET", "/pd/api/v1/labels", "").1, Value::Null);
    for (method, path, body, code) in [
        ("POST", "/pd/api/v1/store/1/label", "[]", 400),
        ("POST", "/pd/api/v1/store/1/label", r#"{"zone":12}"#, 400),
        ("POST", "/pd/api/v1/store/1/label", r#"{"zone":[]}"#, 400),
        ("POST", "/pd/api/v1/store/1/label", "{", 400),
        ("DELETE", "/pd/api/v1/store/1/label", "{}", 400),
        ("DELETE", "/pd/api/v1/store/1/label", "null", 400),
        ("POST", "/pd/api/v1/store/1/weight", "[]", 400),
        ("POST", "/pd/api/v1/store/1/weight", "null", 400),
        (
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":null,"region":1}"#,
            400,
        ),
        ("POST", "/pd/api/v1/store/nope/label", "{}", 400),
        ("POST", "/pd/api/v1/store/999/label", "{}", 500),
        ("DELETE", "/pd/api/v1/store/999/label", r#""zone""#, 500),
        (
            "POST",
            "/pd/api/v1/store/999/weight",
            r#"{"leader":1,"region":1}"#,
            500,
        ),
        (
            "POST",
            "/pd/api/v1/store/1/label",
            r#"{"engine":"tikv"}"#,
            400,
        ),
        ("DELETE", "/pd/api/v1/store/1/label", r#""engine""#, 400),
        ("POST", "/pd/api/v1/store/1/label", r#"{"_bad":"a"}"#, 400),
        (
            "POST",
            "/pd/api/v1/store/1/label",
            r#"{"zone":"$bad"}"#,
            400,
        ),
        ("POST", "/pd/api/v1/store/1/weight", "{}", 400),
        ("POST", "/pd/api/v1/store/1/weight", r#"{"leader":1}"#, 400),
        (
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":-1,"region":1}"#,
            400,
        ),
        (
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":1,"region":"1"}"#,
            400,
        ),
        ("POST", "/pd/api/v1/store/1/label", r#"{"zone":"a"}"#, 200),
        ("POST", "/pd/api/v1/store/1/label", r#"{"rack":"b"}"#, 200),
        ("POST", "/pd/api/v1/store/1/label", r#"{"ZONE":"c"}"#, 200),
        ("POST", "/pd/api/v1/store/1/label", r#"{"rack":""}"#, 200),
        (
            "POST",
            "/pd/api/v1/store/1/label?force=false",
            r#"{"host":"h"}"#,
            200,
        ),
        ("DELETE", "/pd/api/v1/store/1/label", r#""HOST""#, 500),
        ("DELETE", "/pd/api/v1/store/1/label", r#""host""#, 200),
        ("DELETE", "/pd/api/v1/store/1/label", r#""host""#, 500),
        ("POST", "/pd/api/v1/store/1/label", "null", 200),
        ("POST", "/pd/api/v1/store/1/label", r#"{"zone":null}"#, 200),
        (
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":0,"region":2.5}"#,
            200,
        ),
        (
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":3.25,"region":4.5,"extra":true}"#,
            200,
        ),
        (
            "POST",
            "/pd/api/v1/store/1/label",
            r#"{"zone":"final"}"#,
            200,
        ),
    ] {
        let before = projection(url);
        let r = request(url, method, path, body);
        assert_eq!(r.0, code, "{method} {path} {body}: {r:?}");
        let after = projection(url);
        if code != 200 {
            assert_eq!(before, after);
        }
        output.push(json!([method, path, body, r, after]));
    }

    let configured = request(
        url,
        "POST",
        "/pd/api/v1/config/replicate",
        r#"{"enable-placement-rules":"false","location-labels":"zone","strictly-match-label":"true"}"#,
    );
    assert_eq!(configured.0, 200, "{configured:?}");
    for (method, body) in [
        ("DELETE", r#""zone""#),
        ("POST", r#"{"zone":""}"#),
        ("POST", r#"{"rack":"x"}"#),
    ] {
        let before = projection(url);
        let response = request(url, method, "/pd/api/v1/store/1/label", body);
        assert_eq!(response.0, 500);
        assert_eq!(projection(url), before);
        output.push(json!(["strict", method, body, response]));
    }
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"strictly-match-label":"false"}"#
        )
        .0,
        200
    );
    let response = client
        .put_store(pdpb::PutStoreRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 7,
                address: "store7".into(),
                state: 2,
                node_state: 3,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    for (method, path, body) in [
        ("POST", "label", "{}"),
        ("DELETE", "label", r#""zone""#),
        ("POST", "weight", r#"{"leader":1,"region":1}"#),
    ] {
        let response = request(url, method, &format!("/pd/api/v1/store/7/{path}"), body);
        assert_eq!(response.0, 500);
        output.push(json!(["removed", method, path, response]));
    }
    output.push(json!([
        "labels",
        request(url, "GET", "/pd/api/v1/labels", "")
    ]));
    json!(output)
}
fn projection(url: &str) -> Value {
    let v = request(url, "GET", "/pd/api/v1/store/1", "").1;
    let mut labels = v["store"]["labels"].as_array().cloned().unwrap_or_default();
    labels.sort_by_key(|l| l.to_string());
    json!({"labels":labels,"leader":v["status"]["leader_weight"].as_f64(),"region":v["status"]["region_weight"].as_f64()})
}
async fn rust_observe() -> Value {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = restart_listener("127.0.0.1:0".parse().unwrap());
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let config = NodeConfig {
        cluster_id: 753,
        member_id: 1,
        name: "state".into(),
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
    let mut result = observe(&url, 753).await;
    node.shutdown().await.unwrap();
    let listener = restart_listener(address);
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
    let restored = projection(&url);
    assert_eq!(
        restored,
        json!({"labels":[{"key":"zone","value":"final"}],"leader":3.25,"region":4.5})
    );
    result.as_array_mut().unwrap().push(restored);
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_053_labels_weights_and_restart() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_053_differential() {
    let rust = rust_observe().await;
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
    let mut result = observe(&go.url, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    result.as_array_mut().unwrap().push(projection(&go.url));
    go.stop().unwrap();
    println!("Go: {result}\nRust: {rust}");
    assert_eq!(result, rust);
}

// A restart must exercise persisted state, not the platform TCP TIME_WAIT policy.
// REUSEADDR permits rebinding after close; REUSEPORT is deliberately not enabled.
fn restart_listener(address: std::net::SocketAddr) -> tokio::net::TcpListener {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_reuseaddr(true).unwrap();
    socket.bind(address).unwrap();
    socket.listen(128).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_053_fault_cancel_and_forward() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(crate::http_store_state_test::StateStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let config = NodeConfig {
        cluster_id: 7531,
        member_id: 1,
        name: "labels".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    observe(&url, 7531).await;
    let read = Read::prefix(b"/pd/7531/raft/", Consistency::Linearizable);
    let weights = Read::prefix(
        b"/pd/7531/schedule/store_weight/",
        Consistency::Linearizable,
    );
    let weights_before = inner.read(weights.clone()).await.unwrap().entries;
    let before = inner.read(read.clone()).await.unwrap().entries;
    for (path, body) in [
        ("label", r#"{"_bad":"x"}"#),
        ("weight", r#"{"leader":-1,"region":2}"#),
    ] {
        assert_eq!(
            request(&url, "POST", &format!("/pd/api/v1/store/1/{path}"), body).0,
            400
        );
        assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
        assert_eq!(
            inner.read(weights.clone()).await.unwrap().entries,
            weights_before
        );
    }
    let expected = projection(&url);
    store.mode.store(1, Ordering::SeqCst);
    let failed = request(
        &url,
        "POST",
        "/pd/api/v1/store/1/label",
        r#"{"zone":"failed"}"#,
    );
    assert_eq!(failed.0, 500);
    assert!(failed.1.as_str().unwrap().contains("lease"));
    assert_eq!(projection(&url), expected);
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    // Weight writes precede metadata. Metadata failure preserves the old live
    // weights while the independently committed persistent weights survive.
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":8,"region":9}"#
        )
        .0,
        500
    );
    assert_eq!(projection(&url), expected);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(inner.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7531);
    let mut restored = vec![];
    endpoint
        .load_stores(&paths, |s| restored.push(s))
        .await
        .unwrap();
    assert_eq!(restored[0].leader_weight(), 8.0);
    assert_eq!(restored[0].region_weight(), 9.0);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7531,
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
            "POST",
            "/pd/api/v1/store/1/label",
            r#"{"rack":"forwarded"}"#
        )
        .0,
        200
    );
    assert_eq!(projection(&url)["labels"].as_array().unwrap().len(), 2);
    assert_eq!(
        request(
            &follower_url,
            "POST",
            "/pd/api/v1/store/1/weight",
            r#"{"leader":6,"region":7}"#
        )
        .0,
        200
    );
    assert_eq!(projection(&url)["leader"], 6.0);
    assert_eq!(
        request(
            &follower_url,
            "DELETE",
            "/pd/api/v1/store/1/label",
            r#""rack""#
        )
        .0,
        200
    );
    follower.shutdown().await.unwrap();
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
    let pending_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                "POST",
                "--data-binary",
                r#"{"zone":"cancelled"}"#,
                "-w",
                "\n%{http_code}",
                &format!("{pending_url}/pd/api/v1/store/1/label"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let output = pending.await.unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().ends_with("500"));
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}
