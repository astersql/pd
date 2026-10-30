// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_064_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 764,
                member_id: 1,
                name: "scheduler".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = crate::http_member_test::request(&url, "POST", "/pd/api/v1/schedulers", "{}");
    assert_eq!(
        (response.0, response.1),
        (400, serde_json::json!("missing scheduler name")),
        "scheduler management must be routed"
    );
    println!("Rust: {}", observe(&url, 764).await);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(764);
    assert!(
        !endpoint
            .load_scheduler_config(&paths, "evict-leader-scheduler")
            .await
            .unwrap()
            .is_empty()
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                NodeConfig {
                    cluster_id: 764,
                    member_id: 1,
                    name: "scheduler".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    println!("restart: {}", restart(&url, 764).await);
    restarted.shutdown().await.unwrap();
}

use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
use std::time::Duration;
fn request(url: &str, method: &str, path: &str, body: Value) -> (u16, Value) {
    crate::http_member_test::request(
        url,
        method,
        &format!("/pd/api/v1/schedulers{path}"),
        &body.to_string(),
    )
}
fn sorted(mut response: (u16, Value)) -> (u16, Value) {
    if let Some(a) = response.1.as_array_mut() {
        a.sort_by_key(Value::to_string)
    }
    response
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
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    for (method, path, input, code) in [
        ("POST", "", json!({}), 400),
        ("POST", "", json!({"name":1}), 400),
        ("POST", "", json!({"name":"unknown"}), 400),
        ("POST", "", json!({"name":"evict-leader-scheduler"}), 400),
        (
            "POST",
            "",
            json!({"name":"evict-leader-scheduler","store_id":1.5}),
            400,
        ),
        (
            "POST",
            "",
            json!({"name":"evict-leader-scheduler","store_ids":[]}),
            400,
        ),
        (
            "POST",
            "",
            json!({"name":"evict-leader-scheduler","store_id":1,"store_ids":[2]}),
            400,
        ),
        ("GET", "", Value::Null, 500),
        ("DELETE", "/missing", Value::Null, 500),
        ("POST", "/missing", json!({"delay":1}), 500),
    ] {
        let r = request(url, method, path, input.clone());
        assert_eq!(r.0, code, "{input} {r:?}");
        out.push(json!([method, path, input, r]));
    }
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
    let _client = bootstrap(url, id).await;
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
    let mut client = _client.clone();
    let stream = tokio::spawn(async move {
        client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner()
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if request(url, "GET", "", Value::Null)
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
    // Default sets are fixed by the canonical startup policy. Array order is unspecified in Go.
    out.push(json!([
        "defaults",
        sorted(request(url, "GET", "", Value::Null))
    ]));
    for (method, path, input, code) in [
        ("DELETE", "/missing", Value::Null, 404),
        ("POST", "/missing", json!({"delay":10}), 500),
        ("POST", "/all", json!({}), 400),
        ("POST", "/all", json!({"delay":null}), 200),
        ("POST", "/all", json!({"delay":0}), 200),
    ] {
        let r = request(url, method, path, input.clone());
        assert_eq!(r.0, code, "{input} {r:?}");
        out.push(json!([method, path, input, r]));
    }
    for input in [
        json!({"name":"label-scheduler"}),
        json!({"name":"evict-slow-trend-scheduler"}),
        json!({"name":"balance-range-scheduler","rule":"leader-scatter","engine":"tikv","alias":"job","start-key":"a","end-key":"z"}),
        json!({"name":"shuffle-leader-scheduler"}),
        json!({"name":"shuffle-region-scheduler"}),
        json!({"name":"random-merge-scheduler"}),
        json!({"name":"balance-witness-scheduler"}),
        json!({"name":"transfer-witness-leader-scheduler"}),
        json!({"name":"shuffle-hot-region-scheduler","limit":2}),
        json!({"name":"evict-leader-scheduler","store_ids":[1,2,1]}),
        json!({"name":"grant-leader-scheduler","store_id":3}),
        json!({"name":"scatter-range","start_key":"a","end_key":"z","range_name":"test"}),
    ] {
        let before = sorted(request(url, "GET", "", Value::Null));
        let r = request(url, "POST", "", input.clone());
        assert_eq!(
            r,
            (200, json!("The scheduler is created.")),
            "{input}: {r:?}"
        );
        out.push(json!([input, r]));
        let after = sorted(request(url, "GET", "", Value::Null));
        let added: Vec<_> = after
            .1
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| !before.1.as_array().unwrap().contains(v))
            .collect();
        assert_eq!(added.len(), 1, "{before:?} {after:?}");
        let name = added[0].as_str().unwrap();
        if !matches!(
            input["name"].as_str().unwrap(),
            "evict-leader-scheduler" | "grant-leader-scheduler"
        ) {
            let duplicate = request(url, "POST", "", input.clone());
            assert_eq!(duplicate.0, 400);
            out.push(json!(["duplicate", duplicate]));
        }
        let paused = request(url, "POST", &format!("/{name}"), json!({"delay":3600}));
        assert_eq!(paused.0, 200);
        out.push(json!([name, paused]));
        let periods = request(url, "GET", "?status=paused&timestamp=false", Value::Null);
        assert_eq!(periods.0, 200);
        let period = periods
            .1
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap();
        let a = chrono::DateTime::parse_from_rfc3339(period["paused_at"].as_str().unwrap())
            .unwrap()
            .timestamp();
        let b = chrono::DateTime::parse_from_rfc3339(period["resume_at"].as_str().unwrap())
            .unwrap()
            .timestamp();
        assert_eq!(b - a, 3600);
        out.push(json!([
            "paused",
            sorted(request(url, "GET", "?status=paused", Value::Null))
        ]));
        let resumed = request(url, "POST", &format!("/{name}"), json!({"delay":-1}));
        assert_eq!(resumed.0, 200);
        assert_eq!(
            request(url, "GET", "?status=paused", Value::Null),
            (200, json!([]))
        );
        out.push(json!(["resume", resumed]));
        let deleted = request(url, "DELETE", &format!("/{name}"), Value::Null);
        assert_eq!(deleted, (200, json!("The scheduler is removed.")));
        out.push(json!([name, deleted]));
        assert_eq!(sorted(request(url, "GET", "", Value::Null)), before);
    }
    for base in ["evict-leader-scheduler", "grant-leader-scheduler"] {
        for (input, expected) in [
            (
                json!({"name":base,"store_id":1}),
                "The scheduler is created.",
            ),
            (
                json!({"name":base,"store_id":2,"batch":99}),
                "The scheduler has been applied to the store.",
            ),
        ] {
            let result = request(url, "POST", "", input.clone());
            assert_eq!(result, (200, json!(expected)));
            out.push(json!(["update", input, result]));
        }
        for (suffix, code) in [("bad", 400), ("999", 404), ("1", 200), ("2", 200)] {
            let result = request(url, "DELETE", &format!("/{base}-{suffix}"), Value::Null);
            assert_eq!(result, (code, Value::Null));
            out.push(json!([base, suffix, result]));
        }
        assert!(
            !request(url, "GET", "", Value::Null)
                .1
                .as_array()
                .unwrap()
                .contains(&json!(base))
        );
    }
    // Removing and re-adding a default scheduler toggles the persisted startup list.
    for (method, path, input) in [
        ("DELETE", "/balance-leader-scheduler", Value::Null),
        ("POST", "", json!({"name":"balance-leader-scheduler"})),
        ("POST", "/all", json!({"delay":3600})),
    ] {
        let r = request(url, method, path, input.clone());
        assert_eq!(r.0, 200);
        out.push(json!([method, path, input, r]));
    }
    out.push(json!([
        "all-paused",
        sorted(request(url, "GET", "?status=paused", Value::Null))
    ]));
    assert_eq!(request(url, "POST", "/all", json!({"delay":0})).0, 200);
    // Leave a durable non-default policy, paused only in term memory.
    assert_eq!(
        request(
            url,
            "POST",
            "",
            json!({"name":"evict-leader-scheduler","store_id":1})
        )
        .0,
        200
    );
    assert_eq!(
        request(
            url,
            "POST",
            "/evict-leader-scheduler",
            json!({"delay":3600})
        )
        .0,
        200
    );
    drop(tx);
    stream.abort();
    let _ = stream.await;
    json!(out)
}

async fn restart(url: &str, id: u64) -> Value {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
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
    let (commands_tx, mut commands) = tokio::sync::mpsc::channel(8);
    let reader = tokio::spawn(async move {
        let mut stream = client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        while let Some(c) = stream.message().await.unwrap() {
            if c.transfer_leader.is_some() && commands_tx.send(c).await.is_err() {
                break;
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if request(url, "GET", "", Value::Null)
                .1
                .as_array()
                .is_some_and(|a| a.contains(&json!("evict-leader-scheduler")))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap();
    let names = sorted(request(url, "GET", "", Value::Null));
    assert!(
        names
            .1
            .as_array()
            .unwrap()
            .contains(&json!("balance-leader-scheduler")),
        "re-added default must survive restart: {names:?}"
    );
    // Go pause TTLs are volatile. Durable scheduler creation survives restart.
    assert_eq!(
        request(url, "GET", "?status=paused", Value::Null),
        (200, json!([]))
    );
    // Pause all existing policies before enabling the worker; then only resume eviction.
    assert_eq!(request(url, "POST", "/all", json!({"delay":3600})).0, 200);
    assert_eq!(
        crate::http_member_test::request(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"halt-scheduling":"false"}"#
        )
        .0,
        200
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(150), commands.recv())
            .await
            .is_err(),
        "paused policies must not deliver an operator"
    );
    assert_eq!(
        request(url, "POST", "/evict-leader-scheduler", json!({"delay":0})).0,
        200
    );
    let command = tokio::time::timeout(Duration::from_secs(30), commands.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.region_id, 100);
    assert_eq!(command.target_peer.as_ref().unwrap().store_id, 1);
    let destination = command.transfer_leader.unwrap().peer.unwrap();
    assert!([2, 3].contains(&destination.store_id));
    assert_eq!(
        request(url, "DELETE", "/evict-leader-scheduler", Value::Null).0,
        200
    );
    drop(tx);
    reader.abort();
    let _ = reader.await;
    json!([names,"pause-reset",{"region":100,"from":1,"destination-in":[2,3]}])
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_064_differential() {
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
    let oracle_restart = restart(&go.url, id).await;
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7641,
        member_id: 1,
        name: "scheduler".into(),
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
    let rust = observe(&url, 7641).await;
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
    let rust_restart = restart(&url, 7641).await;
    node.shutdown().await.unwrap();
    println!(
        "Go: {oracle}\nRust: {rust}\nRestart Go: {oracle_restart}\nRestart Rust: {rust_restart}"
    );
    assert_eq!(oracle, rust);
    assert_eq!(oracle_restart, rust_restart);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_064_fault_and_follower() {
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
                cluster_id: 7642,
                member_id: 1,
                name: "scheduler".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let _client = bootstrap(&url, 7642).await;
    let config_read = Read::range(b"/pd/7642/config", b"", Consistency::Linearizable);
    let before = inner.read(config_read.clone()).await.unwrap().entries;
    store.mode.store(3, Ordering::SeqCst);
    let failed = request(&url, "POST", "", json!({"name":"label-scheduler"}));
    assert_eq!(failed.0, 400, "{failed:?}");
    assert_eq!(
        before,
        inner.read(config_read.clone()).await.unwrap().entries,
        "failed config transaction must not commit"
    );
    assert!(
        request(&url, "GET", "", Value::Null)
            .1
            .as_array()
            .unwrap()
            .contains(&json!("label-scheduler")),
        "Go retains the admitted scheduler after final option persistence fails"
    );
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    assert!(
        !endpoint
            .load_scheduler_config(
                &astersql_pd_keypath::ClusterPaths::new(7642),
                "label-scheduler"
            )
            .await
            .unwrap()
            .is_empty()
    );
    store.mode.store(3, Ordering::SeqCst);
    assert_eq!(
        request(&url, "DELETE", "/label-scheduler", Value::Null).0,
        500
    );
    assert!(
        request(&url, "GET", "", Value::Null)
            .1
            .as_array()
            .unwrap()
            .contains(&json!("label-scheduler")),
        "failed remove must retain its running policy"
    );
    assert_eq!(
        request(&url, "DELETE", "/label-scheduler", Value::Null).0,
        200
    );
    assert!(
        endpoint
            .load_scheduler_config(
                &astersql_pd_keypath::ClusterPaths::new(7642),
                "label-scheduler"
            )
            .await
            .unwrap()
            .is_empty()
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7642,
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
        request(&follower_url, "DELETE", "/missing", Value::Null),
        (
            404,
            json!("[PD:scheduler:ErrSchedulerNotFound]scheduler not found")
        )
    );
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-H",
            "PD-Allow-follower-handle: true",
            "-w",
            "\n%{http_code}",
            &format!("{follower_url}/pd/api/v1/schedulers"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.ends_with("500") && text.contains("ErrNotBootstrapped"),
        "{text}"
    );
    follower.shutdown().await.unwrap();
    let raft = Read::prefix(b"/pd/7642/raft/", Consistency::Linearizable);
    let before = inner.read(raft.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
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
                r#"{"zone":"cancelled"}"#,
                "-w",
                "\n%{http_code}",
                &format!("{post_url}/pd/api/v1/store/1/label"),
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
    socket.write_all(b"POST /pd/api/v1/schedulers HTTP/1.1\r\nHost: localhost\r\nContent-Length: 26\r\nConnection: close\r\n\r\n{\"name\":\"label-scheduler\"}").await.unwrap();
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
    assert_eq!(before, inner.read(raft).await.unwrap().entries);
}
