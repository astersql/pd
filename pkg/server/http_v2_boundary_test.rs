// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v2/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (
        status.parse().unwrap(),
        serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
    )
}

// Read-only Go registrations are the independent route oracle, rather than a
// duplicate of the Rust registration table. Every concrete Go method is exercised.
fn routes() -> Vec<(String, String, &'static str, &'static str)> {
    let mut routes = vec![];
    let re = regex::Regex::new(r#"router\.(GET|POST|PUT|PATCH|DELETE)\("([^"]*)""#).unwrap();
    let param = regex::Regex::new(r"[:*][A-Za-z_]+").unwrap();
    for (source, prefix, domain, dependency) in [
        (
            include_str!("../../server/apiv2/handlers/keyspace.go"),
            "keyspaces",
            "keyspace",
            "theme-8",
        ),
        (
            include_str!("../../server/apiv2/handlers/safe_point.go"),
            "gc/safepoint",
            "gc",
            "theme-8",
        ),
        (
            include_str!("../../server/apiv2/handlers/tso_keyspace_group.go"),
            "tso/keyspace-groups",
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            include_str!("../../server/apiv2/handlers/meta_service_group.go"),
            "meta-service-groups",
            "meta-service-group",
            "theme-8",
        ),
        (
            include_str!("../../server/apiv2/handlers/microservice.go"),
            "ms",
            "mcs",
            "theme-8",
        ),
        (
            include_str!("../../server/apiv2/handlers/maintenance.go"),
            "maintenance",
            "maintenance",
            "theme-9",
        ),
    ] {
        for m in re.captures_iter(source) {
            let suffix = param.replace_all(&m[2], "1");
            routes.push((
                m[1].into(),
                format!("{prefix}/{}", suffix.trim_start_matches('/'))
                    .trim_end_matches('/')
                    .into(),
                domain,
                dependency,
            ));
        }
    }
    routes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_078_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 778,
                member_id: 1,
                name: "boundary".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    // Real role/cluster assembly, including the existing tonic bootstrap adapter.
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    let response = client
        .bootstrap(astersql_pd_proto::pdpb::BootstrapRequest {
            header: Some(astersql_pd_proto::pdpb::RequestHeader {
                cluster_id: 778,
                ..Default::default()
            }),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                ..Default::default()
            }),
            region: Some(astersql_pd_proto::metapb::Region {
                id: 2,
                peers: vec![astersql_pd_proto::metapb::Peer {
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
    assert!(response.header.unwrap().error.is_none());
    let snapshot = || async {
        let entries = store
            .read(Read::prefix(b"/pd/778/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries;
        entries
            .into_iter()
            .filter(|e| {
                [
                    b"keyspaces".as_slice(),
                    b"gc/",
                    b"maintenance",
                    b"meta_service",
                    b"tso/keyspace",
                ]
                .iter()
                .any(|p| e.key.windows(p.len()).any(|w| w == *p))
            })
            .map(|e| (e.key, e.value))
            .collect::<Vec<_>>()
    };
    store
        .transact(vec![], vec![Mutation::put(b"/pd/778/gc/safe_point", b"42")])
        .await
        .unwrap();
    let before = snapshot().await;
    assert!(!before.is_empty());
    let observations: Vec<_> = routes()
        .into_iter()
        .map(|(method, path, domain, dep)| {
            let result = call(&url, &method, &path, "invalid JSON must not reach a domain");
            (method, path, domain, dep, result)
        })
        .collect();
    let after = snapshot().await;
    let ready = call(&url, "GET", "ready?verbose", "");
    let affinity = call(
        &url,
        "POST",
        "affinity-groups",
        r#"{"affinity_groups":{"boundary":{"ranges":[{}]}}}"#,
    );
    let persisted = store
        .read(Read::prefix(
            b"/pd/778/affinity_groups/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    let unknown = call(&url, "GET", "keyspaces/1/unregistered", "");
    let root = call(&url, "GET", "", "");
    let wrong_method = call(&url, "DELETE", "keyspaces", "");
    let ambiguous: Vec<_> = [
        "keyspaces/id",
        "tso/keyspace-groups/1/split",
        "tso/keyspace-groups/1/merge",
        "tso/keyspace-groups/1/alloc",
        "tso/keyspace-groups/1/keyspaces",
    ]
    .into_iter()
    .map(|p| {
        call(
            &url,
            if p == "keyspaces/id" { "GET" } else { "PATCH" },
            p,
            "{}",
        )
    })
    .collect();
    node.shutdown().await.unwrap();
    assert_eq!(observations.len(), 30);
    assert_eq!(
        before, after,
        "rejected routes must not mutate domain state"
    );
    for (method, path, domain, dep, result) in observations {
        println!("{method} {path}: {result:?}");
        assert_eq!(
            result,
            (
                503,
                json!({"error":"domain_dependency_unavailable", "domain":domain, "dependency":dep, "compatible":false})
            ),
            "{method} {path}"
        );
    }
    assert_eq!(ready.0, 200);
    assert_eq!(
        affinity.0, 200,
        "real affinity remains reachable: {affinity:?}"
    );
    assert_eq!(persisted.len(), 1);
    assert_eq!(unknown.0, 404);
    assert_eq!(root.0, 404);
    assert_eq!(wrong_method.0, 405);
    for result in ambiguous {
        assert_eq!(result.0, 503);
        assert_eq!(result.1["compatible"], false);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_078_cancel_and_unread_body() {
    use std::io::{Read as _, Write};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let stop = CancellationToken::new();
    let app = crate::http_v2_boundary::router(cancel.clone());
    let shutdown = stop.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    // A denied route must not wait for or deserialize an untrusted streaming body.
    socket.write_all(b"POST /pd/api/v2/keyspaces HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\n").unwrap();
    let mut bytes = [0; 4096];
    let count = socket.read(&mut bytes).unwrap();
    let response = String::from_utf8_lossy(&bytes[..count]).into_owned();
    drop(socket);
    cancel.cancel();
    let result = call(
        &format!("http://{address}"),
        "PUT",
        "maintenance/restart/1",
        "{}",
    );
    stop.cancel();
    task.await.unwrap();
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert!(
        response.contains("x-pd-compatibility: unavailable"),
        "{response}"
    );
    assert_eq!(
        result,
        (
            503,
            json!({"error":"request_cancelled", "domain":"maintenance", "dependency":"theme-9", "compatible":false})
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary via scripts/http-v2-boundary-oracle.py"]
async fn task_078_differential() {
    let oracle: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_V2_BOUNDARY_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 7787,
                member_id: 1,
                name: "boundary".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut gaps = vec![];
    for entry in oracle["routes"].as_array().unwrap() {
        let path = entry["path"].as_str().unwrap();
        let method = entry["method"].as_str().unwrap();
        let result = call(&url, method, path, "invalid JSON must not reach a domain");
        gaps.push((entry.clone(), result));
    }
    let available: Vec<_> = oracle["available"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, go)| (path.clone(), go.clone(), call(&url, "GET", path, "")))
        .collect();
    node.shutdown().await.unwrap();
    assert_eq!(gaps.len(), 30);
    for (go, rust) in gaps {
        // Observe both implementations, but never normalize dependency denials
        // into successful Go behavior or call these unavailable domains equivalent.
        println!("intentional domain gap: Go {go}; Rust {rust:?}");
        assert_eq!(rust.0, 503);
        assert_eq!(rust.1["compatible"], false);
        assert_eq!(rust.1["error"], "domain_dependency_unavailable");
        // An unknown MCS service returns JSON 404 from the domain handler.
        assert!(
            go["go"]["content_type"]
                .as_str()
                .unwrap()
                .starts_with("application/json"),
            "Go route is not registered: {go}"
        );
    }
    for (path, go, rust) in available {
        let expected: Value = serde_json::from_str(go["body"].as_str().unwrap()).unwrap();
        assert_eq!(rust.0 as u64, go["status"].as_u64().unwrap(), "{path}");
        assert_eq!(rust.1, expected, "{path}");
        println!("compatible available route {path}: {rust:?}");
    }
}
