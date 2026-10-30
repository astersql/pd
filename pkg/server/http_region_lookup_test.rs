// Copyright 2026 AsterSQL.
fn request(url: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--path-as-is",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}\n%{redirect_url}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, redirect) = text.rsplit_once('\n').unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    let code: u16 = code.parse().unwrap();
    if code == 301 {
        println!("redirect {path}: {body:?} content={content:?}");
        return (
            code,
            json!([
                body,
                content,
                redirect.strip_prefix(url).unwrap_or(redirect)
            ]),
        );
    }
    assert_eq!(
        content,
        if code == 200 && !path.contains("/label/") {
            "application/octet-stream"
        } else {
            "application/json; charset=UTF-8"
        },
        "{path}"
    );
    (code, serde_json::from_str(body).unwrap())
}
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_055_behavior() {
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
    let response = request(&url, "GET", "/pd/api/v1/region/id/1", "");
    node.shutdown().await.unwrap();
    assert_eq!(
        response.0, 500,
        "unbootstrapped store mutation: {response:?}"
    );
}
const LABEL_RULE: &str = r#"{"id":"task055","labels":[{"key":"foo","value":"bar"}],"rule_type":"key-range","data":[{"start_key":"61","end_key":"6D"}]}"#;
async fn observe(url: &str, id: u64, go: bool) -> Value {
    let cold = request(url, "GET", "/pd/api/v1/region/id/1", "");
    assert_eq!(cold.0, 500);
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    if go {
        assert_eq!(
            crate::http_member_test::request(
                url,
                "POST",
                "/pd/api/v1/config/region-label/rule",
                LABEL_RULE
            )
            .0,
            200
        );
    }
    assert_eq!(
        request(url, "GET", "/pd/api/v1/region/id/10/label/foo", "").1,
        "bar"
    );
    let mut result = vec![json!(cold)];
    for path in [
        "/region/id/10/label/foo",
        "/region/id/999/label/foo",
        "/region/id/0/label/foo",
        "/region/id/abc/label/foo",
        "/region/id/10",
        "/region/id/11",
        "/region/id/12",
        "/region/id/999",
        "/region/id/0",
        "/region/id/-1",
        "/region/id/+1",
        "/region/id/a",
        "/region/id/18446744073709551616",
        "/region/key/a",
        "/region/key/m",
        "/region/key/z",
        "/region/key/zz",
        "/region/key/%FF",
        "/region/key/%00",
        "/region/key/%2F",
        "/region/key/%2F?x=y",
        "/region/key/%2E",
        "/region/key/%2E%2E",
        "/region/key/a%2Fb",
        "/region/key/a+b",
        "/region/key/%00?format=hex",
        "/region/key/+",
        "/region/key/%2B",
        "/region/key/%25",
        "/region/key/61?format=hex",
        "/region/key/6D?format=hex",
        "/region/key/6?format=hex",
        "/region/key/gg?format=hex",
        "/region/key/61?format=HEX",
        "/region/key/%2561",
        "/regions/sibling/10",
        "/regions/sibling/11",
        "/regions/sibling/12",
        "/regions/sibling/999",
        "/regions/sibling/0",
        "/regions/sibling/-1",
        "/regions/sibling/+10",
        "/regions/sibling/a",
        "/regions/sibling/+",
        "/regions/sibling/-",
        "/regions/sibling/--1",
        "/regions/sibling/9223372036854775808",
    ] {
        let r = request(url, "GET", &format!("/pd/api/v1{path}"), "");
        println!("{path}: {r:?}");
        if path == "/region/id/10" {
            assert_eq!(r.0, 200);
            assert_eq!(r.1["start_key"], "61");
            assert_eq!(r.1["pending_peers"][0]["is_learner"], true);
            assert_eq!(r.1["down_peers"][0]["down_seconds"], 17);
        }
        result.push(json!([path, r]));
    }
    json!(result)
}
async fn rust_observe() -> Value {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    use astersql_pd_storage::Mutation;
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: astersql_pd_keypath::ClusterPaths::new(755)
                    .region_label_key_path("task055")
                    .into_bytes(),
                value: LABEL_RULE.as_bytes().to_vec(),
                lease: None,
            }],
        )
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 755,
                member_id: 1,
                name: "lookup".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 755, false).await;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = Read::prefix(b"/pd/755/raft/r/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in ["/region/id/10", "/regions/sibling/10", "/region/key/m"] {
        assert_eq!(
            request(&url, "GET", &format!("/pd/api/v1{path}"), "").0,
            200
        );
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_055_point_queries() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_055_differential() {
    let rust = rust_observe().await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let id = PdClient::connect(go.url.clone())
        .await
        .unwrap()
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let result = observe(&go.url, id, true).await;
    go.stop().unwrap();
    println!("Go: {result}\nRust: {rust}");
    assert_eq!(result, rust);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_055_cancel_and_follower() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use std::sync::atomic::{AtomicU8, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(crate::http_store_state_test::StateStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7551,
                member_id: 1,
                name: "lookup".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7551).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7551,
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
        request(&follower_url, "GET", "/pd/api/v1/region/id/10", "").1,
        request(&url, "GET", "/pd/api/v1/region/id/10", "").1
    );
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let out = std::process::Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "3",
                    "-H",
                    "pd-allow-follower-handle: true",
                    &format!("{follower_url}/pd/api/v1/region/id/10"),
                ])
                .output()
                .unwrap();
            if serde_json::from_slice::<Value>(&out.stdout)
                .ok()
                .is_some_and(|v| v["id"] == 10)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    follower.shutdown().await.unwrap();
    let read = Read::prefix(b"/pd/7551/raft/", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
    let post_url = url.clone();
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
                &format!("{post_url}/pd/api/v1/store/1/label"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket
        .write_all(
            b"GET /pd/api/v1/region/id/10 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(150),
            socket.read_to_end(&mut response)
        )
        .await
        .is_err()
    );
    cancel.cancel();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(response.starts_with("HTTP/1.1 500"), "{response}");
    assert!(response.contains("server not started"), "{response}");
    let out = pending.await.unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().ends_with("500"));
    node.shutdown().await.unwrap();
    assert_eq!(before, inner.read(read).await.unwrap().entries);
}
