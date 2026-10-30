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

async fn observe(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let before = request(url, "GET", "/pd/api/v1/stores", "");
    assert_eq!(before.0, 500);
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
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
    assert!(r.header.unwrap().error.is_none());
    let r = request(url, "GET", "/pd/api/v1/store/1", "");
    assert_eq!(r.0, 200, "{r:?}");
    assert_eq!(r.1["store"]["id"], 1);
    assert_eq!(r.1["store"]["state_name"], "Down");
    assert_eq!(r.1["status"]["capacity"], "0B");
    let mut results = vec![("initial".to_owned(), r)];
    for (n, state) in [(4, 0), (5, 1), (6, 2)] {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: n,
                    address: format!("store-{n}:20160"),
                    state,
                    node_state: match state {
                        1 => 2,
                        2 => 3,
                        _ => 1,
                    },
                    labels: vec![metapb::StoreLabel {
                        key: "Zone".into(),
                        value: format!("West{n}"),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(
            response.header.as_ref().unwrap().error.is_none(),
            "{response:?}"
        );
    }
    for path in [
        "/pd/api/v1/store/4",
        "/pd/api/v1/store/5",
        "/pd/api/v1/store/6",
        "/pd/api/v1/store/999",
        "/pd/api/v1/store/nope",
        "/pd/api/v1/store/-1",
        "/pd/api/v1/store/18446744073709551616",
        "/pd/api/v1/stores",
        "/pd/api/v1/stores?state=2",
        "/pd/api/v1/stores?state=0&state=1&state=2",
        "/pd/api/v1/stores?state=1&state=1",
        "/pd/api/v1/stores?state=3",
        "/pd/api/v1/stores?state=4294967296",
        "/pd/api/v1/stores?state=999999999999999999999999999999999999999",
        "/pd/api/v1/store/%2B1",
        "/pd/api/v1/store/%61",
        "/pd/api/v1/labels/stores?value=%5Cd",
        "/pd/api/v1/labels/stores?value=%5CQWest4%5CE",
        "/pd/api/v1/labels/stores?name=%28",
        "/pd/api/v1/labels/stores?name=%29",
        "/pd/api/v1/labels/stores?name=%2A",
        "/pd/api/v1/labels/stores?name=%5Cq",
        "/pd/api/v1/labels/stores?name=%28%3F%3Dx%29",
        "/pd/api/v1/labels/stores?name=%5Bz-a%5D",
        "/pd/api/v1/labels/stores?name=a%7B1001%7D",
        "/pd/api/v1/labels/stores?name=foo%7B",
        "/pd/api/v1/labels/stores?name=%7B001%7D",
        "/pd/api/v1/labels/stores?name=%5Bz%26%26q%5D",
        "/pd/api/v1/labels/stores?name=%5B%5B%5D",
        "/pd/api/v1/labels/stores?name=%5C132one",
        "/pd/api/v1/labels/stores?name=%5Cu005A",
        "/pd/api/v1/labels/stores?name=%5C",
        "/pd/api/v1/stores?state=wat",
        "/pd/api/v1/stores?state=",
        "/pd/api/v1/stores/check",
        "/pd/api/v1/stores/check?state=DoWn&state=offline",
        "/pd/api/v1/stores/check?state=up",
        "/pd/api/v1/stores/check?state=",
        "/pd/api/v1/stores/check?state=bad",
        "/pd/api/v1/labels/stores",
        "/pd/api/v1/labels/stores?name=zone&value=WEST%5B45%5D",
        "/pd/api/v1/labels/stores?name=missing",
        "/pd/api/v1/labels/stores?name=%5B",
    ] {
        let mut observed = request(url, "GET", path, "");
        if let Some(stores) = observed.1.get_mut("stores").and_then(Value::as_array_mut) {
            stores.sort_by_key(|s| s["store"]["id"].as_u64());
        }
        results.push((path.to_owned(), observed));
    }
    assert_eq!(
        results
            .iter()
            .find(|(p, _)| p == "/pd/api/v1/stores")
            .unwrap()
            .1
            .1["count"],
        3
    );
    assert_eq!(
        results
            .iter()
            .find(|(p, _)| p == "/pd/api/v1/labels/stores?name=zone&value=WEST%5B45%5D")
            .unwrap()
            .1
            .1["count"],
        2
    );
    let hb = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header,
            stats: Some(pdpb::StoreStats {
                store_id: 4,
                capacity: 1048576,
                available: 524288,
                used_size: 524288,
                is_busy: true,
                sending_snap_count: 2,
                receiving_snap_count: 3,
                slow_score: 42,
                slow_trend: Some(pdpb::SlowTrend {
                    cause_value: 2.5,
                    cause_rate: 1.5,
                    result_value: 3.5,
                    result_rate: -0.5,
                }),
                network_slow_scores: std::collections::HashMap::from([(5, 9)]),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(hb.header.unwrap().error.is_none());
    let mut updated = request(url, "GET", "/pd/api/v1/store/4", "");
    assert_eq!(updated.0, 200);
    assert_eq!(updated.1["store"]["state_name"], "Up");
    assert_eq!(updated.1["status"]["capacity"], "1MiB");
    let stamp = updated.1["store"]["last_heartbeat"].as_i64().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i128;
    assert!((now - stamp as i128).abs() < 10_000_000_000);
    let time = chrono::DateTime::parse_from_rfc3339(
        updated.1["status"]["last_heartbeat_ts"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(time.timestamp_nanos_opt().unwrap(), stamp);
    let uptime: astersql_pd_typeutil::Duration =
        serde_json::from_value(updated.1["status"]["uptime"].clone()).unwrap();
    assert_eq!(uptime.as_nanos(), stamp);
    // Each process timestamps its own heartbeat; verify the relationships above,
    // then replace only these three related process-clock observations.
    updated.1["store"]["last_heartbeat"] = json!("verified clock");
    updated.1["status"]["last_heartbeat_ts"] = json!("verified clock");
    updated.1["status"]["uptime"] = json!("verified clock");
    results.push(("heartbeat".into(), updated));
    json!({"before":before,"results":results})
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
                cluster_id: 751,
                member_id: 1,
                name: "cluster".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 751).await;
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_051_behavior() {
    rust_observe().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_051_differential() {
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
    let result = observe(&go.url, id).await;
    go.stop().unwrap();
    println!("Go: {result}\nRust: {rust}");
    assert_eq!(normalize(result), normalize(rust));
}

fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 && f >= i64::MIN as f64 && f < i64::MAX as f64 {
                json!(f as i64)
            } else {
                Value::Number(n)
            }
        }
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(&s)
            .map(|t| json!(t.timestamp_nanos_opt().unwrap()))
            .unwrap_or(Value::String(s)),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}

#[test]
fn task_051_clock_and_projection() {
    use astersql_pd_core::{StoreInfo, StoreMeta, StoreState, StoreStats};
    use astersql_pd_typeutil::Duration;
    let mut config = astersql_pd_server_config::Config::default();
    config.schedule.max_store_down_time = Duration::from_nanos(60_000_000_000);
    config.schedule.leader_schedule_policy = "size".into();
    config.schedule.region_score_formula_version = "v2".into();
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        last_heartbeat: 1_000_000_000,
        start_timestamp: 0,
        ..Default::default()
    });
    store.set_new_store_stats(StoreStats {
        capacity: 1024,
        available: 512,
        used_size: 512,
        is_busy: true,
        sending_snap_count: 2,
        receiving_snap_count: 3,
        slow_score: 42,
        ..Default::default()
    });
    for (now, name) in [
        (21_000_000_000, "Up"),
        (21_000_000_001, "Disconnected"),
        (61_000_000_000, "Disconnected"),
        (61_000_000_001, "Down"),
    ] {
        let body = crate::http_store_read::project(&store, &config, now);
        assert_eq!(body["store"]["state_name"], name);
        assert_eq!(body["status"]["capacity"], "1KiB");
        assert_eq!(body["status"]["is_busy"], true);
        assert_eq!(body["status"]["uptime"], "1s");
        assert_eq!(body["status"]["slow_score"], 42);
    }
    store.edit_meta(|m| m.state = StoreState::Offline);
    assert_eq!(
        crate::http_store_read::project(&store, &config, 1_000_000_000_000)["store"]["state_name"],
        "Offline"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_051_follower_persistence_and_cancel() {
    use crate::http_schedule_config_write_fault_test::ConfigStore;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(ConfigStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let mut nodes = vec![];
    let mut urls = vec![];
    for member_id in [1, 2] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(
            tokio::runtime::Handle::current(),
            if member_id == 1 {
                cancel.clone()
            } else {
                CancellationToken::new()
            },
        )
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7512,
                member_id,
                name: format!("stores-{member_id}"),
                client_url: url.clone(),
                peer_url: format!("http://peer-{member_id}"),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    observe(&urls[0], 7512).await;
    let read = Read::prefix(b"/pd/7512/raft/s/", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    assert!(!before.is_empty());
    for path in [
        "/pd/api/v1/store/4",
        "/pd/api/v1/stores?state=2",
        "/pd/api/v1/labels/stores?name=zone",
        "/pd/api/v1/stores/check?state=up",
    ] {
        assert_eq!(
            request(&urls[0], "GET", path, ""),
            request(&urls[1], "GET", path, "")
        );
    }
    assert_eq!(before, inner.read(read.clone()).await.unwrap().entries);
    // A genuine config transaction retains the cluster resource lock. Store
    // reads wait on that same live view, and shutdown must cancel both waiters.
    store.mode.store(2, Ordering::SeqCst);
    let url = urls[0].clone();
    let write = tokio::task::spawn_blocking(move || {
        cancel_request(
            &url,
            "POST",
            "/pd/api/v1/config",
            r#"{"pd-server.flow-round-by-digit":10}"#,
        )
    });
    tokio::time::timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let url = urls[0].clone();
    let pending =
        tokio::task::spawn_blocking(move || cancel_request(&url, "GET", "/pd/api/v1/store/4", ""));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!pending.is_finished());
    cancel.cancel();
    assert_eq!(
        pending.await.unwrap(),
        (
            500,
            "[PD:server:ErrServerNotStarted]server not started".to_owned()
        )
    );
    assert_eq!(write.await.unwrap().0, 500);
    tokio::time::timeout(Duration::from_secs(5), nodes[0].shutdown())
        .await
        .unwrap()
        .unwrap();
    nodes[1].shutdown().await.unwrap();
    assert_eq!(before, inner.read(read).await.unwrap().entries);
    let url = urls[0].clone();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                inner,
                NodeConfig {
                    cluster_id: 7512,
                    member_id: 1,
                    name: "stores-1".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer-1".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    let restored = request(&url, "GET", "/pd/api/v1/stores?state=2", "");
    assert_eq!(restored.0, 200);
    assert_eq!(restored.1["count"], 1);
    assert_eq!(restored.1["stores"][0]["store"]["id"], 6);
    assert_eq!(
        restored.1["stores"][0]["store"]["labels"],
        json!([{"key":"Zone","value":"West6"}])
    );
    restarted.shutdown().await.unwrap();
}

fn cancel_request(url: &str, method: &str, path: &str, body: &str) -> (u16, String) {
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
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.trim().to_owned())
}
