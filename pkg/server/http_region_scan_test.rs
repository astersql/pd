// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_056_behavior() {
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
    let response = crate::http_member_test::request(&url, "GET", "/pd/api/v1/regions/key", "");
    node.shutdown().await.unwrap();
    assert_eq!(response.0, 500, "unbootstrapped region scan: {response:?}");
}

use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
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
        if code == 200 && !path.ends_with("/count") {
            "application/octet-stream"
        } else {
            "application/json; charset=UTF-8"
        },
        "{path}"
    );
    (code, serde_json::from_str(body).unwrap())
}
async fn observe(url: &str, id: u64) -> Value {
    let cold = request(url, "GET", "/pd/api/v1/regions/key", "");
    assert_eq!(cold.0, 500);
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    let mut result = vec![json!(cold)];
    for path in [
        "/regions",
        "/regions/count",
        "/regions/key",
        "/regions/key?limit=1",
        "/regions/key?key=m&limit=1",
        "/regions/key?key=zz&limit=1",
        "/regions/key?key=b&end_key=b",
        "/regions/key?key=z&end_key=zz",
        "/regions/key?key=z",
        "/regions/key?key=zz&end_key=m",
        "/regions/key?key=6D&end_key=7A&format=hex",
        "/regions/key?key=6&format=hex",
        "/regions/key?end_key=gg&format=hex",
        "/regions/key?key=%00&format=hex",
        "/regions/key?key=%FF",
        "/regions/key?key=a+b",
        "/regions/key?key=%2561",
        "/regions/key?limit=0",
        "/regions/key?limit=-1",
        "/regions/key?limit=10241",
        "/regions/key?limit=a",
        "/regions/key?limit=9223372036854775808",
        "/regions/key?limit=%2B1",
        "/regions/key?limit=+1",
        "/regions/key?limit=1&limit=2",
        "/regions/key?key=%GG&limit=1",
        "/regions/key?key=m;limit=1",
        "/regions/store/1",
        "/regions/store/2",
        "/regions/store/3",
        "/regions/store/99",
        "/regions/store/1?type=leader",
        "/regions/store/1?type=follower",
        "/regions/store/2?type=follower",
        "/regions/store/3?type=learner",
        "/regions/store/3?type=pending",
        "/regions/store/3?type=witness",
        "/regions/store/0",
        "/regions/store/-1",
        "/regions/store/+1",
        "/regions/store/a",
        "/regions/store/9223372036854775808",
    ] {
        let mut r = request(url, "GET", &format!("/pd/api/v1{path}"), "");
        // Only the all-region root map has unspecified order; store lists keep
        // Go's leader/follower/learner concatenation and each tree's key order.
        if path == "/regions" {
            r.1["regions"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|v| v["id"].as_u64());
        }
        if path == "/regions/key?limit=1" {
            assert_eq!(r.1["regions"][0]["id"], 10);
            assert_eq!(r.1["count"], 1);
        }
        if path == "/regions/key?key=m&limit=1" {
            assert_eq!(r.1["regions"][0]["id"], 11);
        }
        if path == "/regions/key?key=zz&limit=1" {
            assert_eq!(r.1["regions"][0]["id"], 12);
        }
        println!("{path}: {r:?}");
        result.push(json!([path, r]));
    }
    result.push(large_pages(url, &mut client, id).await);
    // The existing Go binary leaks a subtree read lock on an unknown type;
    // keep this error last so later heartbeat writes do not deadlock the oracle.
    result.push(json!(request(
        url,
        "GET",
        "/pd/api/v1/regions/store/1?type=unknown",
        ""
    )));
    json!(result)
}
async fn rust_observe() -> Value {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 756,
                member_id: 1,
                name: "scan".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 756).await;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = Read::prefix(b"/pd/756/raft/r/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in [
        "/regions",
        "/regions/key?limit=1",
        "/regions/count",
        "/regions/store/1",
    ] {
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
async fn task_056_scan_queries() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_056_differential() {
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
    let result = observe(&go.url, id).await;
    go.stop().unwrap();
    println!("Go: {result}\nRust: {rust}");
    assert_eq!(result, rust);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_056_cancel_and_follower() {
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
        request(&follower_url, "GET", "/pd/api/v1/regions/key?limit=1", "").1,
        request(&url, "GET", "/pd/api/v1/regions/key?limit=1", "").1
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
                    &format!("{follower_url}/pd/api/v1/regions/key?limit=1"),
                ])
                .output()
                .unwrap();
            if serde_json::from_slice::<Value>(&out.stdout)
                .ok()
                .is_some_and(|v| v["regions"][0]["id"] == 10)
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
            b"GET /pd/api/v1/regions/key?limit=1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
#[tokio::test]
async fn task_056_stream_pages_cancel_and_drop() {
    use astersql_pd_core::{RegionBucketMeta, RegionInfo, RegionMeta};
    use http_body_util::BodyExt;
    let regions: Vec<_> = (0..130)
        .map(|id| {
            let mut r = RegionInfo::new(
                RegionMeta {
                    id,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            );
            if id == 63 {
                r.set_bucket_meta(Some(RegionBucketMeta {
                    version: 9,
                    keys: vec![vec![255]],
                }));
            }
            Arc::new(r)
        })
        .collect();
    let bytes = crate::http_region_scan::stream_regions(regions.clone(), CancellationToken::new())
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let expected = astersql_pd_response::marshal_regions_info_json(
        &regions.iter().map(|r| Some(r.as_ref())).collect::<Vec<_>>(),
        || None,
    )
    .unwrap();
    assert_eq!(bytes.as_ref(), expected);
    let cancel = CancellationToken::new();
    let mut body = crate::http_region_scan::stream_regions(regions.clone(), cancel.clone());
    let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert!(!first.ends_with(b"]}"));
    cancel.cancel();
    assert!(body.frame().await.unwrap().is_err());
    assert!(body.frame().await.is_none());
    drop(body);
    let last = Arc::downgrade(regions.last().unwrap());
    let mut body = crate::http_region_scan::stream_regions(regions, CancellationToken::new());
    body.frame().await.unwrap().unwrap();
    assert!(last.upgrade().is_some());
    drop(body);
    assert!(
        last.upgrade().is_none(),
        "disconnect must release unread snapshot pages"
    );
}
async fn large_pages(
    url: &str,
    client: &mut PdClient<tonic::transport::Channel>,
    id: u64,
) -> Value {
    use crate::grpc_region_lookup_scenarios_test::header;
    use astersql_pd_proto::metapb;
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(150);
    for i in 0..130 {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: 1000 + i,
                start_key: if i == 0 {
                    vec![]
                } else {
                    format!("{i:04}").into_bytes()
                },
                end_key: if i == 129 {
                    vec![]
                } else {
                    format!("{:04}", i + 1).into_bytes()
                },
                peers: vec![peer.clone()],
                region_epoch: Some(metapb::RegionEpoch {
                    version: 3,
                    conf_ver: 1,
                }),
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let r = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: header(id),
                    region_id: 1129,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if r.region.is_some() && r.leader == Some(peer.clone()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    drop(stream);
    let mut result = vec![];
    for (path, count) in [
        ("/regions", 130),
        ("/regions/key", 16),
        ("/regions/key?limit=0", 130),
        ("/regions/key?limit=-1", 130),
        ("/regions/store/1", 130),
        ("/regions/count", 130),
    ] {
        let mut r = request(url, "GET", &format!("/pd/api/v1{path}"), "");
        assert_eq!(r.1["count"], count, "{path}");
        if path == "/regions" {
            r.1["regions"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|r| r["id"].as_u64());
        }
        result.push(json!([path, r]));
    }
    // End-key continuation through three output pages, with no duplicated or
    // omitted region. Empty final end key terminates rather than restarting.
    let mut key = String::new();
    let mut ids = vec![];
    loop {
        let r = request(
            url,
            "GET",
            &format!("/pd/api/v1/regions/key?format=hex&key={key}&limit=64"),
            "",
        );
        let page = r.1["regions"].as_array().unwrap();
        ids.extend(page.iter().map(|r| r["id"].as_u64().unwrap()));
        key = page.last().unwrap()["end_key"].as_str().unwrap().into();
        result.push(json!(["page", r]));
        if key.is_empty() {
            break;
        }
    }
    assert_eq!(ids, (1000..1130).collect::<Vec<_>>());
    json!(result)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_056_tcp_disconnect_releases_unread_pages() {
    use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // A large real Region payload keeps Hyper under TCP backpressure. The weak
    // reference observes payload lifetime, not mocked scan invocation counts.
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: vec![b'a'; 64 * 1024],
                end_key: vec![],
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ));
    let weak = Arc::downgrade(&region);
    let regions = Arc::new(std::sync::Mutex::new(Some(vec![region; 512])));
    let observed = Arc::new(std::sync::Mutex::new(None));
    let scope = observed.clone();
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get(move || {
                let regions = regions.clone();
                let scope = scope.clone();
                async move {
                    let cancel = crate::request_context::current().unwrap().cancel;
                    *scope.lock().unwrap() = Some(cancel.clone());
                    crate::http_region_scan::stream_regions(
                        regions.lock().unwrap().take().unwrap(),
                        cancel,
                    )
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            crate::request_context::ContextState::new(CancellationToken::new()),
            crate::request_context::middleware,
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let stopping = shutdown.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(stopping.cancelled_owned())
            .await
            .unwrap();
    });
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = [0; 1024];
    let n = socket.read(&mut bytes).await.unwrap();
    assert!(n > 0);
    assert!(String::from_utf8_lossy(&bytes[..n]).starts_with("HTTP/1.1 200"));
    assert!(weak.upgrade().is_some());
    drop(socket);
    let observed = observed.lock().unwrap().take().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), observed.cancelled())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    shutdown.cancel();
    server.await.unwrap();
}
