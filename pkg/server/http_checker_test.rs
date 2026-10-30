// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_066_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 766,
                member_id: 1,
                name: "checker".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let observations = observe(&url, 766).await;
    println!("Rust: {observations}");
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 766,
                member_id: 1,
                name: "checker".into(),
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
        checker(&url, "GET", "merge", ""),
        (200, json!({"paused":false}))
    );
    assert_eq!(
        req(&url, "GET", "/pd/api/v1/config/schedule", "").1["enable-diagnostic"],
        "true"
    );
    node.shutdown().await.unwrap();
}
fn req(url: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    crate::http_member_test::request(url, method, path, body)
}
fn checker(url: &str, method: &str, name: &str, body: &str) -> (u16, Value) {
    req(url, method, &format!("/pd/api/v1/checker/{name}"), body)
}
fn diagnostic(url: &str, name: &str) -> (u16, Value) {
    req(
        url,
        "GET",
        &format!("/pd/api/v1/schedulers/diagnostic/{name}"),
        "",
    )
}
async fn wait_diagnostic(url: &str, status: &str) -> (u16, Value) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let mut r = diagnostic(url, "balance-leader-scheduler");
        if r.0 == 200 && r.1["status"] == status {
            assert!(r.1["timestamp"].as_u64().unwrap() > 0);
            r.1.as_object_mut().unwrap().remove("timestamp");
            return r;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "expected {status}: {r:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    out.push(json!(["cold-checker", checker(url, "GET", "merge", "")]));
    out.push(json!([
        "cold-diagnostic",
        diagnostic(url, "balance-leader-scheduler")
    ]));
    out.push(json!([
        "cold-unsupported",
        diagnostic(url, "shuffle-leader-scheduler")
    ]));
    let client = bootstrap(url, id).await;
    for name in [
        "joint-state",
        "split",
        "learner",
        "replica",
        "rule",
        "affinity",
        "merge",
    ] {
        let r = checker(url, "GET", name, "");
        assert_eq!(r, (200, json!({"paused":false})));
        out.push(json!([name, r]));
        let r = checker(url, "POST", name, r#"{"delay":3600}"#);
        assert_eq!(r.0, 200);
        out.push(json!([name, r]));
        let r = checker(url, "GET", name, "");
        assert_eq!(r, (200, json!({"paused":true})));
        out.push(json!([name, r]));
        let r = checker(url, "POST", name, r#"{"delay":0}"#);
        assert_eq!(r.0, 200);
        out.push(json!([name, r]));
        assert_eq!(
            checker(url, "GET", name, ""),
            (200, json!({"paused":false}))
        );
    }
    for body in [
        "{}",
        "null",
        r#"{"delay":-1}"#,
        r#"{"delay":1.5}"#,
        r#"{"delay":"1"}"#,
        r#"{"delay":null}"#,
        r#"{"delay":1,"unused":true}"#,
        "{",
        "[]",
    ] {
        out.push(json!([body, checker(url, "POST", "merge", body)]));
    }
    out.push(json!(["unknown", checker(url, "GET", "missing", "")]));
    out.push(json!([
        "unknown-pause",
        checker(url, "POST", "missing", r#"{"delay":1}"#)
    ]));
    out.push(json!([
        "unsupported",
        diagnostic(url, "shuffle-leader-scheduler")
    ]));
    assert_eq!(
        req(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"enable-diagnostic":"false"}"#
        )
        .0,
        200
    );
    out.push(json!([
        "disabled",
        diagnostic(url, "balance-leader-scheduler")
    ]));
    assert_eq!(
        req(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"enable-diagnostic":"true","halt-scheduling":"true"}"#
        )
        .0,
        200
    );
    gate(url, id, client).await;
    out.push(json!(["halted", wait_diagnostic(url, "halted").await]));
    assert_eq!(
        req(
            url,
            "POST",
            "/pd/api/v1/schedulers/balance-leader-scheduler",
            r#"{"delay":3600}"#
        )
        .0,
        200
    );
    assert_eq!(
        req(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"halt-scheduling":"false"}"#
        )
        .0,
        200
    );
    out.push(json!(["paused", wait_diagnostic(url, "paused").await]));
    assert_eq!(
        req(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"leader-schedule-limit":0}"#
        )
        .0,
        200
    );
    out.push(json!(["pending", wait_diagnostic(url, "pending").await]));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let mut r = diagnostic(url, "balance-region-scheduler");
        if r.0 == 200
            && matches!(
                r.1["status"].as_str(),
                Some("normal" | "pending" | "scheduling")
            )
        {
            r.1.as_object_mut().unwrap().remove("timestamp");
            out.push(json!(["region-plans", r]));
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "real region diagnostic: {r:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(
        req(
            url,
            "DELETE",
            "/pd/api/v1/schedulers/balance-leader-scheduler",
            ""
        )
        .0,
        200
    );
    let mut r = diagnostic(url, "balance-leader-scheduler");
    assert_eq!(r.0, 200);
    assert_eq!(r.1["status"], "disabled");
    r.1.as_object_mut().unwrap().remove("timestamp");
    out.push(json!(["removed", r]));
    assert_eq!(checker(url, "POST", "merge", r#"{"delay":3600}"#).0, 200);
    json!(out)
}

async fn bootstrap(url: &str, id: u64) -> PdClient<tonic::transport::Channel> {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                node_state: 1,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 301,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    for store_id in 1..=3 {
        let r = client
            .put_store(pdpb::PutStoreRequest {
                header: crate::grpc_region_lookup_scenarios_test::header(id),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("store:{store_id}"),
                    version: "8.5.0".into(),
                    node_state: 1,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.unwrap().error.is_none());
        let r = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: crate::grpc_region_lookup_scenarios_test::header(id),
                stats: Some(pdpb::StoreStats {
                    store_id,
                    capacity: 1 << 40,
                    available: 1 << 39,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.unwrap().error.is_none());
    }
    client
}

async fn gate(url: &str, id: u64, client: PdClient<tonic::transport::Channel>) {
    // Trigger the same preparation gate with a complete heartbeat snapshot.
    let peers: Vec<_> = (1..=3)
        .map(|store_id| metapb::Peer {
            id: 300 + store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: crate::grpc_region_lookup_scenarios_test::header(id),
        region: Some(metapb::Region {
            id: 100,
            peers: peers.clone(),
            region_epoch: Some(metapb::RegionEpoch {
                version: 3,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peers[0].clone()),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut client = client.clone();
    let stream = tokio::spawn(async move {
        client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner()
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if crate::http_member_test::request(url, "GET", "/pd/api/v1/schedulers", "")
                .1
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    drop(tx);
    stream.abort();
    let _ = stream.await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_066_differential() {
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
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7661,
                member_id: 1,
                name: "checker".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&url, 7661).await;
    node.shutdown().await.unwrap();
    println!("Go: {oracle}\nRust: {rust}");
    assert_eq!(normalize_summary(oracle), normalize_summary(rust));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_066_follower_and_cancel() {
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
                cluster_id: 7662,
                member_id: 1,
                name: "checker".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    bootstrap(&url, 7662).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7662,
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
        checker(&follower_url, "POST", "merge", r#"{"delay":3600}"#).0,
        200
    );
    assert_eq!(
        checker(&url, "GET", "merge", ""),
        (200, json!({"paused":true}))
    );
    assert_eq!(
        diagnostic(&follower_url, "balance-leader-scheduler"),
        diagnostic(&url, "balance-leader-scheduler")
    );
    follower.shutdown().await.unwrap();
    // Checker state is volatile; its update writes no persistent configuration.
    let read = Read::range(b"/pd/7662/config", b"", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    assert_eq!(checker(&url, "POST", "merge", r#"{"delay":0}"#).0, 200);
    assert_eq!(before, inner.read(read.clone()).await.unwrap().entries);
    // Hold a real config transaction to queue the checker command behind it.
    // The same term cancellation must release both requests without committing.

    store.mode.store(4, Ordering::SeqCst);
    let post_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                "POST",
                "--data-binary",
                r#"{"halt-scheduling":"true"}"#,
                "-w",
                "\n%{http_code}",
                &format!("{post_url}/pd/api/v1/config/schedule"),
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    });
    tokio::time::timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.write_all(b"POST /pd/api/v1/checker/merge HTTP/1.1\r\nHost: localhost\r\nContent-Length: 14\r\nConnection: close\r\n\r\n{\"delay\":3600}").await.unwrap();
    let mut response = vec![];
    assert!(
        tokio::time::timeout(
            Duration::from_millis(150),
            socket.read_to_end(&mut response)
        )
        .await
        .is_err()
    );
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8(response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 500") && response.contains("server not started"),
        "{response}"
    );
    assert!(pending.await.unwrap().ends_with("500"));
    node.shutdown().await.unwrap();
    assert_eq!(before, inner.read(read).await.unwrap().entries);
    println!("follower pause observed on leader; config unchanged; cancellation: {response}");
}
#[test]
fn task_066_pause_deadline() {
    let p = astersql_pd_schedule_checker::PauseController::default();
    p.pause_or_resume_at(10, 100);
    assert!(p.is_paused_at(109));
    assert!(!p.is_paused_at(110));
    p.pause_or_resume_at(20, 100);
    p.pause_or_resume_at(0, 105);
    assert!(!p.is_paused_at(105));
}

// Go aggregates summary entries in a map, so presentation order is unspecified.
// Keep each complete reason/count entry; only canonicalize their order.
fn normalize_summary(mut rows: Value) -> Value {
    for row in rows.as_array_mut().unwrap() {
        if let Some(summary) = row
            .get_mut(1)
            .and_then(|r| r.get_mut(1))
            .and_then(|body| body.get_mut("summary"))
        {
            let text = summary.as_str().unwrap();
            if text.ends_with("; ") {
                let mut entries = text.split_terminator("; ").collect::<Vec<_>>();
                entries.sort_unstable();
                *summary = json!(format!("{}; ", entries.join("; ")));
            }
        }
    }
    rows
}
