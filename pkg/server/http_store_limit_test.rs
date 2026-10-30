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
async fn task_054_behavior() {
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
    let response = request(&url, "POST", "/pd/api/v1/store/1/limit", "{}");
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
    let r = request(url, "POST", "/pd/api/v1/store/1/limit", "{}");
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

    let initial = projection(url);
    assert_eq!(initial, json!({}));
    output.push(json!(["bootstrap_limits", initial]));
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    for (store_id, labels) in [
        (
            4,
            vec![metapb::StoreLabel {
                key: "zone".into(),
                value: "z".into(),
            }],
        ),
        (
            5,
            vec![
                metapb::StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                },
                metapb::StoreLabel {
                    key: "zone".into(),
                    value: "x".into(),
                },
            ],
        ),
    ] {
        let r = client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("store-{store_id}"),
                    node_state: 1,
                    labels,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.as_ref().unwrap().error.is_none(), "{r:?}");
    }
    for (path, body, code) in [
        ("/store/1/limit", "{}", 400),
        ("/store/1/limit", "null", 400),
        ("/store/1/limit", "[]", 400),
        ("/store/1/limit", "{", 400),
        ("/store/1/limit", r#"{"rate":0}"#, 400),
        ("/store/1/limit", r#"{"rate":null}"#, 400),
        ("/store/1/limit", r#"{"rate":1e309}"#, 400),
        ("/store/1/limit", r#"{"rate":1e-999}"#, 400),
        ("/store/1/limit", r#"{"rate":-1}"#, 400),
        ("/store/1/limit", r#"{"rate":"1"}"#, 400),
        ("/store/1/limit", r#"{"rate":1,"type":null}"#, 400),
        ("/store/1/limit", r#"{"rate":1,"type":"invalid"}"#, 400),
        ("/store/999/limit", "{", 500),
        ("/store/nope/limit", "{}", 400),
        ("/store/nope/limit?ttlSecond=1", "{}", 400),
        ("/stores/limit?ttlSecond=1", "{}", 400),
        ("/stores/limit", r#"{"rate":1,"labels":null}"#, 400),
        ("/stores/limit", r#"{"rate":1,"labels":{"zone":2}}"#, 400),
        ("/stores/limit", r#"{"rate":1,"labels":{"_bad":"x"}}"#, 400),
        ("/store/1/limit", r#"{"rate":12.5}"#, 200),
        ("/store/1/limit", r#"{"rate":20,"type":""}"#, 200),
        (
            "/store/1/limit",
            r#"{"rate":30,"type":"transfer-leader-in"}"#,
            200,
        ),
        (
            "/store/1/limit",
            r#"{"rate":30,"type":"send-snapshot"}"#,
            200,
        ),
        (
            "/stores/limit",
            r#"{"rate":40.5,"type":"remove-peer"}"#,
            200,
        ),
        ("/stores/limit", r#"{"rate":9,"labels":{}}"#, 200),
        (
            "/stores/limit",
            r#"{"rate":8,"labels":{"engine":"tikv"}}"#,
            200,
        ),
    ] {
        let r = request(url, "POST", &format!("/pd/api/v1{path}"), body);
        assert_eq!(r.0, code, "{path} {body}: {r:?}");
        output.push(json!([path, body, r]));
    }
    for (path, code) in [
        ("/stores/limit", 200),
        ("/stores/limit?include_tombstone=true", 200),
        ("/stores/limit?include_tombstone=no", 400),
        ("/stores/progress", 400),
        ("/stores/progress?id=no", 400),
        ("/stores/progress?id=999", 404),
        ("/stores/progress?action=missing", 404),
    ] {
        let r = request(url, "GET", &format!("/pd/api/v1{path}"), "");
        assert_eq!(r.0, code, "{path}: {r:?}");
        output.push(json!([path, r]));
    }
    assert_eq!(projection(url)["1"]["add-peer"].as_f64(), Some(8.0));
    assert_eq!(projection(url)["1"]["remove-peer"].as_f64(), Some(8.0));
    assert_eq!(
        projection(url)["1"]["transfer-leader-in"].as_f64(),
        Some(30.0)
    );
    assert_eq!(projection(url)["4"]["add-peer"].as_f64(), Some(8.0));
    assert_ne!(projection(url)["5"]["add-peer"].as_f64(), Some(8.0));
    let r = request(
        url,
        "POST",
        "/pd/api/v1/stores/limit",
        r#"{"rate":17,"type":"add-peer","labels":{"zone":"z","engine":"tiflash"}}"#,
    );
    assert_eq!(r.0, 200);
    output.push(json!(r));
    let limits = projection(url);
    assert_eq!(limits["1"]["add-peer"].as_f64(), Some(8.0));
    assert_eq!(limits["4"]["add-peer"].as_f64(), Some(17.0));
    assert_eq!(limits["5"]["add-peer"].as_f64(), Some(17.0));
    output.push(limits);
    assert_eq!(
        request(url, "DELETE", "/pd/api/v1/store/4?force=true", "").0,
        200
    );
    tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let r = request(url, "GET", "/pd/api/v1/store/4", "");
            if r.1["store"]["state"] == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let r = request(url, "POST", "/pd/api/v1/store/4/limit", r#"{"rate":23}"#);
    assert_eq!(r.0, 500);
    output.push(json!(r));
    for version in ["v2", "v1"] {
        assert_eq!(
            request(
                url,
                "POST",
                "/pd/api/v1/config/schedule",
                &format!(r#"{{"store-limit-version":"{version}"}}"#)
            )
            .0,
            200
        );
        if version == "v2" {
            for (method, path) in [
                ("GET", "/stores/limit"),
                ("POST", "/stores/limit"),
                ("POST", "/store/nope/limit"),
            ] {
                let r = request(url, method, &format!("/pd/api/v1{path}"), "{}");
                assert_eq!(r.0, 400);
                output.push(json!(r));
            }
        }
    }
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }],
            region_epoch: Some(metapb::RegionEpoch {
                version: 1,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        }),
        approximate_size: 100 * 1024 * 1024,
        ..Default::default()
    })
    .await
    .unwrap();
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let r = request(url, "GET", "/pd/api/v1/store/1", "");
            if r.1["status"]["region_size"].as_i64() == Some(100) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        request(url, "DELETE", "/pd/api/v1/store/1?force=true", "").0,
        200
    );
    let progress = tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let r = request(url, "GET", "/pd/api/v1/stores/progress?id=1", "");
            if r.0 == 200 {
                break r;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(progress.1["action"], "removing");
    assert_eq!(progress.1["progress"].as_f64(), Some(0.0));
    output.push(json!(["progress", progress]));
    output.push(json!([
        "average",
        request(url, "GET", "/pd/api/v1/stores/progress?action=removing", "")
    ]));
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }],
            region_epoch: Some(metapb::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        }),
        approximate_size: 50 * 1024 * 1024,
        ..Default::default()
    })
    .await
    .unwrap();
    let advanced = tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let r = request(url, "GET", "/pd/api/v1/stores/progress?id=1", "");
            if r.1["progress"].as_f64() == Some(0.5) {
                break r;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(advanced.0, 200);
    let speed = advanced.1["current_speed"].as_f64().unwrap();
    assert!(speed > 0.0);
    assert!((advanced.1["left_seconds"].as_f64().unwrap() * speed - 50.0).abs() < 1e-9);
    output.push(json!(["advanced", advanced]));
    let mut configured =
        request(url, "GET", "/pd/api/v1/config/schedule", "").1["store-limit"].clone();
    configured["99"] = json!({"add-peer":19,"remove-peer":20,"transfer-leader-in":21});
    assert_eq!(
        request(
            url,
            "POST",
            "/pd/api/v1/config/schedule",
            &json!({"store-limit":configured}).to_string()
        )
        .0,
        200
    );
    assert!(projection(url).get("99").is_none());
    let included = request(
        url,
        "GET",
        "/pd/api/v1/stores/limit?include_tombstone=true",
        "",
    );
    assert_eq!(included.1["99"]["add-peer"].as_f64(), Some(19.0));
    output.push(json!(["include_unregistered", included]));
    drop(stream);
    drop(tx);
    json!(output)
}
fn projection(url: &str) -> Value {
    request(url, "GET", "/pd/api/v1/stores/limit", "").1
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
    let cluster = astersql_pd_core::BasicCluster::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster.clone())
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
    let store_info = cluster.stores.get_store(1).unwrap();
    assert_eq!(
        store_info
            .limiter()
            .unwrap()
            .read()
            .unwrap()
            .rate(astersql_pd_storelimit::Type::ADD_PEER),
        8.0 / 60.0
    );
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
    assert_eq!(restored["1"]["add-peer"].as_f64(), Some(8.0));
    result.as_array_mut().unwrap().push(restored);
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_054_limits_and_restart() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_054_differential() {
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
    fn numeric_progress(v: &mut Value) {
        match v {
            Value::Array(a) => a.iter_mut().for_each(numeric_progress),
            Value::Object(m) => {
                for (k, v) in m {
                    if ["progress", "current_speed", "left_seconds"].contains(&k.as_str()) {
                        if let Some(n) = v.as_f64() {
                            *v = json!(n);
                        }
                    } else {
                        numeric_progress(v)
                    }
                }
            }
            _ => {}
        }
    }
    let mut rust = rust;
    numeric_progress(&mut rust);
    numeric_progress(&mut result);
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
async fn task_054_fault_cancel_and_forward() {
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
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7541,
                member_id: 1,
                name: "limits".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    observe(&url, 7541).await;
    let read = Read::prefix(b"/pd/7541/config", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    let expected = projection(&url);
    store.mode.store(3, Ordering::SeqCst);
    let r = request(&url, "POST", "/pd/api/v1/store/1/limit", r#"{"rate":99}"#);
    assert_eq!(r.0, 500);
    assert!(r.1.as_str().unwrap().contains("lease"));
    assert_eq!(projection(&url), expected);
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                inner.clone(),
                NodeConfig {
                    cluster_id: 7541,
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
            "/pd/api/v1/store/1/limit",
            r#"{"rate":22}"#
        )
        .0,
        200
    );
    assert_eq!(projection(&url)["1"]["add-peer"].as_f64(), Some(22.0));
    follower.shutdown().await.unwrap();
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(4, Ordering::SeqCst);
    let pending = tokio::task::spawn_blocking(move || {
        std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "10",
                "-w",
                "%{http_code}",
                "-X",
                "POST",
                "-d",
                r#"{"rate":77}"#,
                &format!("{url}/pd/api/v1/store/1/limit"),
            ])
            .output()
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let out = pending.await.unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().ends_with("500"));
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}
