// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_041_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 741,
                    member_id: id,
                    name: format!("redirect-{id}"),
                    client_url: url.clone(),
                    peer_url: format!("http://127.0.0.1:{}", 2400 + id),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        urls.push(url);
        nodes.push(node);
    }
    let fetch = |url: &str| {
        let output = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-w",
                "\n%{http_code}",
                &format!("{url}/pd/api/v1/members?keep=a%2Fb"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    let leader = fetch(&urls[0]);
    let follower = fetch(&urls[1]);
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
    assert_eq!(
        follower, leader,
        "follower must proxy the real leader HTTP response"
    );
    assert!(leader.ends_with("200"));
}

use crate::{Identity, PdServices};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::{Request, StatusCode},
    response::IntoResponse,
};
use prost::Message;
use serde_json::{Value, json};
use std::time::Duration;

struct Follower {
    identity: Identity,
    store: Arc<dyn MetaStore>,
}
#[async_trait::async_trait]
impl PdServices for Follower {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Err("not leader".into())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("not leader".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("not leader".into())
    }
}
async fn publish(store: &Arc<dyn MetaStore>, urls: Vec<String>) {
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/741/leader",
                &pdpb::Member {
                    member_id: 1,
                    name: "upstream".into(),
                    client_urls: urls,
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
}
async fn serve(
    router: axum::Router,
    cancel: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await
        .unwrap();
    });
    (url, handle)
}
async fn follower(
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let services = Arc::new(Follower {
        identity: Identity {
            cluster_id: 741,
            member_id: 2,
            name: "follower".into(),
            client_url: "http://unused".into(),
            peer_url: "http://unused".into(),
        },
        store,
    });
    serve(crate::router(services, cancel.clone()), cancel).await
}
fn curl(url: &str, method: &str, data: Option<&str>, headers: &[&str]) -> (u16, String) {
    let mut command = std::process::Command::new("curl");
    command.args([
        "-sS",
        "--max-time",
        "8",
        "-X",
        method,
        "-w",
        "\n%{http_code}",
    ]);
    for header in headers {
        command.args(["-H", header]);
    }
    if let Some(data) = data {
        command.args(["--data-binary", data]);
    }
    let result = command.arg(url).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let (body, status) = output.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into())
}
#[derive(Clone)]
struct Upstream {
    store: Arc<dyn MetaStore>,
    seen: tokio::sync::mpsc::UnboundedSender<CancellationToken>,
}
async fn observe(
    State(state): State<Upstream>,
    request: Request<Body>,
) -> axum::response::Response {
    if request.uri().path().ends_with("/wait") {
        state
            .seen
            .send(crate::request_context::current().unwrap().cancel)
            .unwrap();
        std::future::pending::<()>().await;
    }
    if request.uri().path().ends_with("/bodywait") {
        use futures_util::StreamExt;
        state
            .seen
            .send(crate::request_context::current().unwrap().cancel)
            .unwrap();
        let stream = futures_util::stream::once(async {
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"first chunk"))
        })
        .chain(futures_util::stream::pending());
        return Body::from_stream(stream).into_response();
    }
    if request.uri().path().ends_with("/gzip") {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(b"compressed leader response").unwrap();
        return (
            StatusCode::BAD_REQUEST,
            [("content-encoding", "gzip")],
            encoder.finish().unwrap(),
        )
            .into_response();
    }
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    let value = json!({"method":parts.method.as_str(), "uri":parts.uri.to_string(), "body":String::from_utf8(bytes.to_vec()).unwrap(), "redirector":parts.headers.get("pd-redirector").unwrap().to_str().unwrap(), "forwarded":parts.headers.get_all("x-forwarded-for").iter().map(|v|v.to_str().unwrap()).collect::<Vec<_>>()});
    state
        .store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/task041/observed",
                &serde_json::to_vec(&value).unwrap(),
            )],
        )
        .await
        .unwrap();
    (
        StatusCode::IM_A_TEAPOT,
        [("x-upstream", "preserved")],
        value.to_string(),
    )
        .into_response()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_041_stream_errors_and_cancellation() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let router = axum::Router::new()
        .fallback(observe)
        .with_state(Upstream {
            store: store.clone(),
            seen: tx,
        })
        .layer(axum::middleware::from_fn_with_state(
            crate::request_context::ContextState::new(cancel.clone()),
            crate::request_context::middleware,
        ));
    let (upstream, server) = serve(router, cancel.clone()).await;
    publish(&store, vec!["http://127.0.0.1:1".into(), upstream]).await;
    let (url, proxy) = follower(store.clone(), cancel.clone()).await;
    let (status, body) = curl(
        &format!("{url}/pd/api/v1/custom/a%2Fb?q=a%2Bb&x=1&x=2"),
        "PATCH",
        Some("binary\nbody ✓"),
        &["X-Forwarded-For: 203.0.113.7", "X-Forwarded-Port: 321"],
    );
    assert_eq!(status, 418);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["method"], "PATCH");
    assert_eq!(value["uri"], "/pd/api/v1/custom/a%2Fb?q=a%2Bb&x=1&x=2");
    assert_eq!(value["body"], "binary\nbody ✓");
    assert_eq!(value["redirector"], "follower");
    assert_eq!(value["forwarded"], json!(["203.0.113.7", "203.0.113.7"]));
    let read = Read::range(b"/task041/observed", b"", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&before.entries[0].value).unwrap(),
        value
    );
    for path in ["/pd/api/v1/members", "/pd/api/v2/unknown"] {
        let result = curl(
            &format!("{url}{path}"),
            "GET",
            None,
            &["PD-Redirector: prior"],
        );
        assert_eq!(result.0, 500);
        assert!(result.1.contains("ErrRedirectToNotLeader"));
    }
    assert_eq!(
        curl(
            &format!("{url}/pd/api/v1/ping"),
            "GET",
            None,
            &["PD-Redirector: prior"]
        )
        .0,
        200
    );
    assert_eq!(
        curl(
            &format!("{url}/pd/api/v1/members"),
            "GET",
            None,
            &["PD-Allow-follower-handle: true"]
        )
        .0,
        503
    );
    assert_eq!(
        curl(&format!("{url}/pd/api/v1/gzip"), "GET", None, &[]),
        (400, "compressed leader response".into())
    );
    // Disconnect an actual HTTP client while the upstream handler is pending.
    use tokio::io::AsyncWriteExt;
    let mut socket = tokio::net::TcpStream::connect(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    socket
        .write_all(b"GET /pd/api/v1/wait HTTP/1.1\r\nHost: test\r\n\r\n")
        .await
        .unwrap();
    let token = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    drop(socket);
    tokio::time::timeout(Duration::from_secs(5), token.cancelled())
        .await
        .unwrap();
    let mut socket = tokio::net::TcpStream::connect(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    socket
        .write_all(b"GET /pd/api/v1/bodywait HTTP/1.1\r\nHost: test\r\n\r\n")
        .await
        .unwrap();
    let token = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    use tokio::io::AsyncReadExt;
    let mut bytes = [0; 1024];
    assert!(
        tokio::time::timeout(Duration::from_secs(5), socket.read(&mut bytes))
            .await
            .unwrap()
            .unwrap()
            > 0
    );
    drop(socket);
    tokio::time::timeout(Duration::from_secs(5), token.cancelled())
        .await
        .unwrap();
    assert_eq!(store.read(read).await.unwrap().entries, before.entries);
    publish(&store, vec![url.clone()]).await;
    assert_eq!(
        curl(&format!("{url}/pd/api/v1/members"), "GET", None, &[]).0,
        500
    );
    publish(&store, vec!["http://127.0.0.1:1".into()]).await;
    assert!(
        curl(&format!("{url}/pd/api/v1/members"), "GET", None, &[])
            .1
            .contains("ErrRedirect]redirect failed")
    );
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: b"/pd/741/leader".to_vec(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        curl(&format!("{url}/pd/api/v2/unknown"), "GET", None, &[]),
        (503, "\"[PD:server:ErrLeaderNil]leader is nil\"".into())
    );
    // Host shutdown must interrupt the v1 backoff and drain the request.
    let mut socket = tokio::net::TcpStream::connect(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    socket
        .write_all(b"GET /pd/api/v1/members HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    cancel.cancel();
    drop(socket);
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), proxy)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires scripts/http-redirect-oracle.py and an existing Go binary"]
async fn task_041_readonly_go_differential() {
    let record: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_HTTP_REDIRECT_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    publish(&store, vec![record["leader"].as_str().unwrap().into()]).await;
    let (rust, server) = follower(store, cancel.clone()).await;
    let go = record["follower"].as_str().unwrap();
    let name = record["before"]["leader"]["name"].as_str().unwrap();
    let cases = vec![
        (
            "/pd/api/v1/members?key=a%2Fb&key=2".to_owned(),
            "GET",
            None,
            vec![],
        ),
        (
            format!("/pd/api/v1/members/name/{name}"),
            "POST",
            Some("{"),
            vec![],
        ),
        (
            format!("/pd/api/v1/members/name/{name}"),
            "POST",
            Some("{\"leader-priority\": 17}"),
            vec![],
        ),
        (
            "/pd/api/v1/members".into(),
            "GET",
            None,
            vec!["PD-Redirector: prior"],
        ),
        (
            "/pd/api/v1/ping".into(),
            "GET",
            None,
            vec!["PD-Redirector: prior"],
        ),
    ];
    let mut cases = cases;
    cases.push((
        "/pd/api/v2/keyspaces".into(),
        "GET",
        None,
        vec!["PD-Redirector: prior"],
    ));
    for (path, method, data, headers) in cases {
        let expected = curl(&format!("{go}{path}"), method, data, &headers);
        if data == Some("{\"leader-priority\": 17}") {
            assert_eq!(
                curl(
                    &format!("{}{path}", record["leader"].as_str().unwrap()),
                    "POST",
                    Some("{\"leader-priority\": 0}"),
                    &[]
                )
                .0,
                200
            );
        }
        let actual = curl(&format!("{rust}{path}"), method, data, &headers);
        println!("oracle {method} {path}: Go={expected:?} Rust={actual:?}");
        assert_eq!(actual, expected);
    }
    cancel.cancel();
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn task_041_leader_wait_clock() {
    let store: Arc<dyn MetaStore> = Arc::new(astersql_pd_storage::MemoryStore::default());
    let services = Arc::new(Follower {
        identity: Identity {
            cluster_id: 741,
            member_id: 2,
            name: "follower".into(),
            client_url: "http://unused".into(),
            peer_url: "http://unused".into(),
        },
        store: store.clone(),
    });
    let start = tokio::time::Instant::now();
    assert!(
        crate::http_redirect::wait_for_leader(services.as_ref(), true)
            .await
            .is_none()
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert!(
        crate::http_redirect::wait_for_leader(services.as_ref(), false)
            .await
            .is_none()
    );
    assert_eq!(start.elapsed(), Duration::from_secs(3));
    let delayed = services.clone();
    let waiting = tokio::spawn(async move {
        crate::http_redirect::wait_for_leader(delayed.as_ref(), false).await
    });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(100)).await;
    publish(&store, vec!["http://127.0.0.1:1234".into()]).await;
    assert_eq!(waiting.await.unwrap().unwrap().name, "upstream");
}
