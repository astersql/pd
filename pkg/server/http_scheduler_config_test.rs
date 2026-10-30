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
async fn task_065_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 765,
                member_id: 1,
                name: "scheduler-config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let r = crate::http_member_test::request(
        &url,
        "GET",
        "/pd/api/v1/scheduler-config/balance-leader-scheduler/list",
        "",
    );
    assert_eq!(
        r.0, 406,
        "config routes must reject unbootstrapped requests"
    );
    println!("Rust observations: {}", observe(&url, 765).await);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let bytes = endpoint
        .load_scheduler_config(
            &astersql_pd_keypath::ClusterPaths::new(765),
            "balance-leader-scheduler",
        )
        .await
        .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap()["batch"], 7);
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 765,
                member_id: 1,
                name: "scheduler-config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let restart = restart_snapshot(&url).await;
    assert_eq!(restart[0][1][1]["batch"], 7);
    println!("restart: {restart}");
    node.shutdown().await.unwrap();
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
async fn prepare(url: &str, id: u64) {
    assert_eq!(
        crate::http_member_test::request(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"halt-scheduling":"true"}"#
        )
        .0,
        200
    );
    let client = bootstrap(url, id).await;
    gate(url, id, client).await;
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
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(ref n)
            if n.as_f64()
                .is_some_and(|f| f.fract() == 0. && f >= 0. && f < 1e15) =>
        {
            json!(n.as_f64().unwrap() as u64)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}
fn raw(url: &str, method: &str, path: &str, body: &str) -> (u16, Value, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "--path-as-is",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    (
        code.parse().unwrap(),
        normalize(serde_json::from_str(body).unwrap_or(json!(body))),
        content.into(),
    )
}
fn config(url: &str, name: &str, method: &str, path: &str, body: &str) -> (u16, Value, String) {
    raw(
        url,
        method,
        &format!("/pd/api/v1/scheduler-config/{name}{path}"),
        body,
    )
}
async fn observe(url: &str, id: u64) -> Value {
    let cold = config(url, "balance-leader-scheduler", "GET", "/list", "");
    assert_eq!(cold.0, 406);
    let options = config(url, "label-scheduler", "OPTIONS", "/list", "");
    assert_eq!(options.0, 200);
    prepare(url, id).await;
    let mut out = vec![json!(["cold", cold]), json!(["options", options])];
    for name in [
        "balance-leader-scheduler",
        "balance-region-scheduler",
        "balance-hot-region-scheduler",
        "evict-slow-store-scheduler",
        "evict-stopping-store-scheduler",
        "balance-witness-scheduler",
        "evict-leader-scheduler",
        "grant-leader-scheduler",
        "label-scheduler",
        "random-merge-scheduler",
        "scatter-range-scheduler",
        "shuffle-hot-region-scheduler",
        "shuffle-leader-scheduler",
        "shuffle-region-scheduler",
        "transfer-witness-leader-scheduler",
        "evict-slow-trend-scheduler",
        "balance-range-scheduler",
        "grant-hot-region-scheduler",
    ] {
        if name == "grant-hot-region-scheduler" {
            assert_eq!(
                raw(
                    url,
                    "POST",
                    "/pd/api/v1/schedulers/all",
                    r#"{"delay":3600}"#
                )
                .0,
                200
            );
            assert_eq!(
                raw(
                    url,
                    "POST",
                    "/pd/api/v1/schedulers/balance-hot-region-scheduler",
                    r#"{"delay":0}"#
                )
                .0,
                200
            );
            assert_eq!(
                raw(
                    url,
                    "POST",
                    "/pd/api/v1/config/schedule",
                    r#"{"halt-scheduling":"false"}"#
                )
                .0,
                200
            );
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let r = config(url, "balance-hot-region-scheduler", "GET", "/list", "");
                    if r.1["read-priorities"] == json!(["query", "byte"]) {
                        out.push(json!(["hot-after-real-tick", r]));
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                raw(
                    url,
                    "POST",
                    "/pd/api/v1/config/schedule",
                    r#"{"halt-scheduling":"true"}"#
                )
                .0,
                200
            );
        }
        if name == "grant-hot-region-scheduler" {
            assert_eq!(
                raw(
                    url,
                    "DELETE",
                    "/pd/api/v1/schedulers/balance-hot-region-scheduler",
                    ""
                )
                .0,
                200
            );
        }
        let mut create = json!({"name":name});
        match name {
            "evict-leader-scheduler" | "grant-leader-scheduler" => create["store_id"] = json!(1),
            "scatter-range-scheduler" => {
                create["name"] = json!("scatter-range");
                create["start_key"] = json!("a");
                create["end_key"] = json!("z");
                create["range_name"] = json!("task065");
            }
            "balance-range-scheduler" => {
                create["rule"] = json!("leader-scatter");
                create["engine"] = json!("tikv");
                create["alias"] = json!("task065");
                create["start-key"] = json!("a");
                create["end-key"] = json!("z");
            }
            "grant-hot-region-scheduler" => {
                create["store-leader-id"] = json!("1");
                create["store-id"] = json!("2,3");
            }
            _ => {}
        }
        let names = raw(url, "GET", "/pd/api/v1/schedulers", "").1;
        if !names.as_array().unwrap().contains(&json!(name)) {
            let r = raw(url, "POST", "/pd/api/v1/schedulers", &create.to_string());
            assert_eq!(r.0, 200, "create {name}: {r:?}");
        }
        let name = if name == "scatter-range-scheduler" {
            "scatter-range-scheduler-task065"
        } else {
            name
        };
        let base = config(url, name, "GET", "/list", "");
        assert_eq!(base.0, 200, "list {name}: {base:?}");
        // Job timestamps are clock-owned. All other fields and responses compare exactly.
        if name != "balance-range-scheduler" {
            out.push(json!([name, "before", base]));
        }
        for (method, path, body) in [
            ("GET", "/config", ""),
            ("PATCH", "/config", "{}"),
            ("POST", "/config", "{"),
            ("POST", "/missing", "{}"),
            ("POST", "/config", "{}"),
        ] {
            let r = config(url, name, method, path, body);
            out.push(json!([name, method, path, body, r]));
        }
        let patch = match name {
            "balance-leader-scheduler" | "balance-witness-scheduler" => r#"{"batch":7}"#,
            "evict-leader-scheduler" => r#"{"store_ids":[1,2],"batch":7}"#,
            "grant-leader-scheduler" => r#"{"store_id":2}"#,
            "evict-slow-store-scheduler" => r#"{"batch":5,"recovery-duration":12}"#,
            "evict-stopping-store-scheduler" => r#"{"batch":5}"#,
            "evict-slow-trend-scheduler" => r#"{"recovery-duration":12}"#,
            "scatter-range-scheduler-task065" => r#"{"start-key":"b","end-key":"y"}"#,
            "shuffle-hot-region-scheduler" => r#"{"limit":7}"#,
            "grant-hot-region-scheduler" => r#"{"store-leader-id":"1","store-id":"2,3"}"#,
            "balance-hot-region-scheduler" => r#"{"min-hot-byte-rate":777}"#,
            "shuffle-region-scheduler" => r#"["leader","follower"]"#,
            _ => "{}",
        };
        let path = if name == "shuffle-region-scheduler" {
            "/roles"
        } else {
            "/config"
        };
        let r = config(url, name, "POST", path, patch);
        assert_eq!(
            r.0,
            if name == "balance-range-scheduler" {
                400
            } else {
                200
            },
            "{name}: {r:?}"
        );
        out.push(json!([name, "update", patch, r]));
        if name == "balance-range-scheduler" {
            for (method, path, body) in [
                (
                    "PUT",
                    "/job",
                    r#"{"engine":"tikv","rule":"leader-scatter","alias":"second","start-key":"b","end-key":"y"}"#,
                ),
                ("DELETE", "/job?job-id=bad", ""),
                ("DELETE", "/job?job-id=1", ""),
                ("DELETE", "/job?job-id=0", ""),
            ] {
                let r = config(url, name, method, path, body);
                out.push(json!([name, method, path, r]));
            }
            let jobs = config(url, name, "GET", "/list", "");
            assert_eq!(jobs.0, 200);
            assert!(
                jobs.1
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["alias"] == "second")
            );
            let summary: Vec<_> = jobs
                .1
                .as_array()
                .unwrap()
                .iter()
                .map(|v| json!([v["job-id"], v["alias"], v["status"], v["ranges"]]))
                .collect();
            out.push(json!([name, "jobs", summary]));
        }
        let after = config(url, name, "GET", "/list", "");
        assert_eq!(after.0, 200);
        if name != "balance-range-scheduler" {
            out.push(json!([name, "after", after]));
        }
        if ["evict-leader-scheduler", "grant-leader-scheduler"].contains(&name) {
            for path in ["/delete/bad", "/delete/999", "/delete/1", "/delete/2"] {
                let r = config(url, name, "DELETE", path, "");
                out.push(json!([name, path, r]));
            }
            assert_eq!(config(url, name, "GET", "/list", "").0, 404);
        }
    }
    for (name, path) in [("missing", "/list"), ("balance-leader-scheduler", "")] {
        let r = config(url, name, "GET", path, "");
        out.push(json!([name, path, r]));
    }
    println!("observations={}", out.len());
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_065_differential() {
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
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let oracle_restart = restart_snapshot(&go.url).await;
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7651,
                member_id: 1,
                name: "scheduler-config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&url, 7651).await;
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7651,
                member_id: 1,
                name: "scheduler-config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust_restart = restart_snapshot(&url).await;
    node.shutdown().await.unwrap();
    assert_eq!(oracle_restart, rust_restart);
    println!("restart: {rust_restart}");
    println!("Go: {oracle}\nRust: {rust}");
    let a = oracle.as_array().unwrap();
    let b = rust.as_array().unwrap();
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a, b);
    }
}

async fn restart_snapshot(url: &str) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    for store_id in 1..=3 {
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
    gate(url, id, client).await;

    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if config(url, "balance-leader-scheduler", "GET", "/list", "").0 == 200 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let mut out = vec![];
    for name in [
        "balance-leader-scheduler",
        "balance-witness-scheduler",
        "evict-slow-store-scheduler",
        "evict-stopping-store-scheduler",
        "shuffle-hot-region-scheduler",
        "shuffle-region-scheduler",
        "scatter-range-scheduler-task065",
        "evict-slow-trend-scheduler",
        "grant-hot-region-scheduler",
    ] {
        let r = config(url, name, "GET", "/list", "");
        assert_eq!(r.0, 200, "restart {name}: {r:?}");
        out.push(json!([name, r]));
    }
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_065_fault_cancel_and_follower() {
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
                cluster_id: 7652,
                member_id: 1,
                name: "scheduler-config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    prepare(&url, 7652).await;
    let read = Read::prefix(b"/pd/7652/scheduler_config/", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(6, Ordering::SeqCst);
    let r = config(
        &url,
        "balance-leader-scheduler",
        "POST",
        "/config",
        r#"{"batch":9}"#,
    );
    assert_eq!(r.0, 200);
    assert_eq!(
        config(&url, "balance-leader-scheduler", "GET", "/list", "").1["batch"],
        9
    );
    assert_eq!(
        before,
        inner.read(read.clone()).await.unwrap().entries,
        "failed transaction cannot commit, but Go retains published leader config"
    );
    assert_eq!(
        config(
            &url,
            "balance-leader-scheduler",
            "POST",
            "/config",
            r#"{"batch":8}"#
        )
        .0,
        200
    );
    assert_eq!(
        raw(
            &url,
            "POST",
            "/pd/api/v1/schedulers",
            r#"{"name":"shuffle-hot-region-scheduler"}"#
        )
        .0,
        200
    );
    let before_shuffle = config(&url, "shuffle-hot-region-scheduler", "GET", "/list", "");
    store.mode.store(6, Ordering::SeqCst);
    assert_eq!(
        config(
            &url,
            "shuffle-hot-region-scheduler",
            "POST",
            "/config",
            r#"{"limit":9}"#
        )
        .0,
        500
    );
    assert_eq!(
        before_shuffle,
        config(&url, "shuffle-hot-region-scheduler", "GET", "/list", "")
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7652,
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
        config(
            &follower_url,
            "balance-leader-scheduler",
            "GET",
            "/list",
            ""
        )
        .1["batch"],
        8
    );
    follower.shutdown().await.unwrap();
    assert_eq!(
        raw(
            &url,
            "POST",
            "/pd/api/v1/schedulers",
            r#"{"name":"evict-leader-scheduler","store_id":1}"#
        )
        .0,
        200
    );
    store.mode.store(3, Ordering::SeqCst);
    let failed = config(&url, "evict-leader-scheduler", "DELETE", "/delete/1", "");
    assert_eq!(failed.0, 500);
    let retained = config(&url, "evict-leader-scheduler", "GET", "/list", "");
    assert_eq!(retained.0, 200);
    assert!(
        !retained.1["store-id-ranges"]
            .as_object()
            .unwrap()
            .is_empty(),
        "{retained:?}"
    );
    assert_eq!(
        config(&url, "evict-leader-scheduler", "DELETE", "/delete/1", "").0,
        200
    );
    assert_eq!(
        config(&url, "evict-leader-scheduler", "GET", "/list", "").0,
        404
    );
    store.mode.store(7, Ordering::SeqCst);
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let body = r#"{"batch":10}"#;
    let request = format!(
        "POST /pd/api/v1/scheduler-config/balance-leader-scheduler/config HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let before = inner.read(read.clone()).await.unwrap().entries;
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
    node.shutdown().await.unwrap();
    assert_eq!(before, inner.read(read).await.unwrap().entries);
}
