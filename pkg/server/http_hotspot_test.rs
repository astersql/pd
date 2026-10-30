// Copyright 2026 AsterSQL.
use crate::grpc_region_lookup_scenarios_test::header;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn request(url: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "8",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/hotspot{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{path}: {out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    let code: u16 = code.parse().unwrap();
    let value = if content == "text/plain; charset=utf-8" {
        assert_eq!(code, 500, "{path}: {body}");
        assert!(
            body.starts_with(
                "PANIC: runtime error: invalid memory address or nil pointer dereference\n"
            ),
            "{path}: {body}"
        );
        // Only Go's outer recovery appends a stack. Keep raw output as evidence.
        println!("recovered error {path}: {body}");
        json!(body.lines().next().unwrap())
    } else {
        assert_eq!(content, "application/json; charset=UTF-8", "{path}: {body}");
        serde_json::from_str(body).unwrap()
    };
    (code, value)
}
// Go omits the decimal point for whole float rates. Canonicalize only exact
// whole f64 values inside 2^53; integer IDs/counters and fractional rates stay exact.
fn numeric_spelling(value: &mut Value) {
    match value {
        Value::Object(map) => map.values_mut().for_each(numeric_spelling),
        Value::Array(values) => values.iter_mut().for_each(numeric_spelling),
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0. && f.abs() <= (1u64 << 53) as f64 {
                *value = json!(f as i64);
            }
        }
        _ => {}
    }
}
fn record(out: &mut Vec<Value>, url: &str, path: &str) -> (u16, Value) {
    let mut r = request(url, path);
    numeric_spelling(&mut r.1);
    // Read timestamps are host arrival times. Validate RFC3339, then compare
    // all other fields exactly; write timestamps come from the fixed report.
    if path.starts_with("/regions/read") && r.0 == 200 {
        for population in ["as_peer", "as_leader"] {
            for stat in r.1[population].as_object_mut().unwrap().values_mut() {
                if let Some(stats) = stat.get_mut("statistics").and_then(Value::as_array_mut) {
                    for peer in stats {
                        let time = peer["last_update_time"].as_str().unwrap();
                        assert!(chrono::DateTime::parse_from_rfc3339(time).is_ok(), "{time}");
                        peer["last_update_time"] = json!("<host-arrival-time>");
                    }
                }
            }
        }
    }
    out.push(json!([path, r]));
    r
}
async fn wait(url: &str, path: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let r = request(url, path);
            assert_eq!(r.0, 200, "{path}: {r:?}");
            if predicate(&r.1) {
                return r.1;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("waiting {path}: {:?}", request(url, path)))
}
async fn observe(url: &str, id: u64) -> Value {
    let mut out = Vec::new();
    for path in [
        "/stores",
        "/buckets",
        "/regions/read",
        "/regions/write",
        "/regions/write?store_id=bad",
        "/regions/read?store_id=1&store_id=bad",
    ] {
        let r = record(&mut out, url, path);
        assert_eq!(
            r.0,
            if path.ends_with("=bad") && !path.contains("store_id=1") {
                400
            } else {
                500
            }
        );
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let region = metapb::Region {
        id: 2,
        peers: vec![peer.clone()],
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        ..Default::default()
    };
    let r = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(region.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    for (store, engine) in [(2, ""), (4, "tiflash_compute")] {
        assert!(
            client
                .put_store(pdpb::PutStoreRequest {
                    header: header(id),
                    store: Some(metapb::Store {
                        id: store,
                        address: format!("store-{store}"),
                        version: "8.5.0".into(),
                        labels: if engine.is_empty() {
                            vec![]
                        } else {
                            vec![metapb::StoreLabel {
                                key: "engine".into(),
                                value: engine.into(),
                            }]
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .header
                .unwrap()
                .error
                .is_none()
        );
    }
    for path in [
        "/stores",
        "/buckets",
        "/regions/read",
        "/regions/write",
        "/regions/write?store_id=2&store_id=2",
        "/regions/read?store_id=0",
        "/regions/read?store_id=999&store_id=bad",
        "/regions/read?store_id=bad&store_id=999",
        "/regions/write?store_id=+1",
        "/regions/write?store_id=%2B1",
        "/regions/write?store_id=-1",
        "/regions/read?store_id=18446744073709551616",
        "/regions/read?store_id=",
        "/regions/write?store_id=1;store_id=bad",
        "/regions/write?store_id=%ZZ",
        "/regions/write?store_id=%FF",
        "/buckets?region_id=bad",
        "/buckets?region_id=18446744073709551616",
        "/buckets?region_id=0&region_id=2",
    ] {
        record(&mut out, url, path);
    }
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    // Send before awaiting response headers; the Go bidi stream is lazy.
    let report = |n: u64, hot: bool, region: metapb::Region, leader: metapb::Peer| {
        pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(region),
            leader: Some(leader),
            interval: Some(pdpb::TimeInterval {
                start_timestamp: 1000 + n * 60,
                end_timestamp: 1060 + n * 60,
            }),
            approximate_size: (n + 1) * 1048576,
            approximate_keys: n * 10000,
            bytes_written: if hot { 60 * 1048576 } else { 0 },
            keys_written: if hot { 60000 } else { 0 },
            query_stats: Some(pdpb::QueryStats {
                put: if hot { 6000 } else { 0 },
                ..Default::default()
            }),
            ..Default::default()
        }
    };
    tx.send(report(0, true, region.clone(), peer.clone()))
        .await
        .unwrap();
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    for n in 1..=20 {
        tx.send(report(n, true, region.clone(), peer.clone()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let response = std::process::Command::new("curl")
                    .args([
                        "-sS",
                        "--max-time",
                        "5",
                        &format!("{url}/pd/api/v1/region/id/2"),
                    ])
                    .output()
                    .unwrap();
                assert!(response.status.success());
                let region: Value = serde_json::from_slice(&response.stdout).unwrap();
                if region["approximate_keys"] == n * 10000 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // Real store reports populate both the store rates and the read peer cache.
        for store_id in [1, 4] {
            assert!(
                client
                    .store_heartbeat(pdpb::StoreHeartbeatRequest {
                        header: header(id),
                        stats: Some(pdpb::StoreStats {
                            store_id,
                            interval: Some(pdpb::TimeInterval {
                                start_timestamp: n * 60,
                                end_timestamp: (n + 1) * 60
                            }),
                            capacity: 1000000000,
                            available: 900000000,
                            bytes_read: 60 * 1048576,
                            keys_read: 60000,
                            bytes_written: 120 * 1048576,
                            keys_written: 120000,
                            query_stats: Some(pdpb::QueryStats {
                                get: 6000,
                                put: 12000,
                                ..Default::default()
                            }),
                            peer_stats: if store_id == 1 {
                                vec![pdpb::PeerStat {
                                    region_id: 2,
                                    read_bytes: 60 * 1048576,
                                    read_keys: 60000,
                                    query_stats: Some(pdpb::QueryStats {
                                        get: 6000,
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }]
                            } else {
                                vec![]
                            },
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner()
                    .header
                    .unwrap()
                    .error
                    .is_none()
            );
        }
        if n == 8 {
            assert_eq!(
                record(&mut out, url, "/regions/read").1["as_peer"]["1"]["regions_count"],
                0,
                "read heat must reach six times the configured degree"
            );
        }
    }
    for direction in ["write", "read"] {
        let path = format!("/regions/{direction}");
        wait(url, &path, |v| v["as_peer"]["1"]["regions_count"] == 1).await;
        let r = record(&mut out, url, &path);
        assert_eq!(r.1["as_peer"]["1"]["statistics"][0]["flow_bytes"], 1048576);
        record(
            &mut out,
            url,
            &format!("{path}?store_id=1&store_id=2&store_id=1"),
        );
        record(&mut out, url, &format!("{path}?store_id=0001"));
    }
    let r = record(&mut out, url, "/stores");
    assert_eq!(r.1["bytes-read-rate"]["1"], 1048576);
    assert!(r.1["bytes-read-rate"].get("4").is_none());
    for n in 1..=10 {
        // The unary stream never acknowledges admitted reports: leave it open
        // while an HTTP read observes the FIFO worker, then cancel the stream.
        let (btx, brx) = tokio::sync::mpsc::channel(1);
        btx.send(pdpb::ReportBucketsRequest {
            header: header(id),
            buckets: Some(metapb::Buckets {
                region_id: 2,
                version: n,
                keys: vec![b"a".to_vec(), b"z".to_vec()],
                period_in_ms: 1000,
                stats: Some(metapb::BucketStats {
                    read_bytes: vec![if n <= 5 { 100000 } else { 0 }],
                    read_keys: vec![if n <= 5 { 10000 } else { 0 }],
                    write_bytes: vec![if n <= 5 { 200000 } else { 0 }],
                    write_keys: vec![if n <= 5 { 20000 } else { 0 }],
                    read_qps: vec![0],
                    write_qps: vec![0],
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
        let mut c = client.clone();
        let task = tokio::spawn(async move {
            c.report_buckets(tokio_stream::wrappers::ReceiverStream::new(brx))
                .await
        });
        if (3..=5).contains(&n) {
            wait(url, "/buckets", |v| v["2"][0]["hot_degree"] == n).await;
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        drop(btx);
        let _ = task.await;
        if n == 5 {
            for path in [
                "/buckets",
                "/buckets?region_id=2&region_id=2",
                "/buckets?region_id=bad",
                "/buckets?region_id=2&region_id=bad",
                "/buckets?region_id=999",
            ] {
                record(&mut out, url, path);
            }
        }
    }
    wait(url, "/buckets", |v| v.as_object().unwrap().is_empty()).await;
    assert_eq!(record(&mut out, url, "/buckets").1, json!({}));
    for n in 21..=36 {
        tx.send(report(n, false, region.clone(), peer.clone()))
            .await
            .unwrap();
    }
    wait(url, "/regions/write", |v| {
        v["as_peer"]["1"]["regions_count"] == 0
    })
    .await;
    record(&mut out, url, "/regions/write");
    // A new membership expires the old read/write peers without a wall-clock sleep.
    let new_peer = metapb::Peer {
        id: 5,
        store_id: 2,
        ..Default::default()
    };
    let mut moved = region;
    moved.peers = vec![new_peer.clone()];
    moved.region_epoch.as_mut().unwrap().conf_ver = 2;
    tx.send(report(37, false, moved, new_peer)).await.unwrap();
    wait(url, "/regions/read", |v| {
        v["as_peer"]["1"]["regions_count"] == 0
    })
    .await;
    record(&mut out, url, "/regions/read");
    drop(tx);
    drop(stream);
    json!(out)
}
async fn rust_observe() -> Value {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 760,
                member_id: 1,
                name: "hotspot".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 760).await;
    let read = Read::prefix(b"/pd/760/raft/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in ["/stores", "/buckets", "/regions/read", "/regions/write"] {
        assert_eq!(request(&url, path).0, 200);
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
    let restored_before = store
        .read(Read::prefix(b"/pd/760/raft/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    // A new serving term restores Store membership, but not volatile rate windows.
    // Reuse the persisted member URL: changing identity is correctly rejected.
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let restored_url = url.clone();
    let mut restored =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 760,
                    member_id: 1,
                    name: "hotspot".into(),
                    client_url: restored_url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restored.ready().await.unwrap();
    let rates = request(&restored_url, "/stores").1;
    assert_eq!(rates["bytes-read-rate"]["1"].as_f64(), Some(0.));
    assert_eq!(rates["bytes-read-rate"]["2"].as_f64(), Some(0.));
    assert!(rates["bytes-read-rate"].get("4").is_none());
    assert_eq!(
        request(&restored_url, "/regions/read").1["as_peer"]["1"]["regions_count"],
        0
    );
    restored.shutdown().await.unwrap();
    assert_eq!(
        restored_before,
        store
            .read(Read::prefix(b"/pd/760/raft/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_060_behavior() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_060_differential() {
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
async fn task_060_cancel_and_follower() {
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
                cluster_id: 7601,
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7601).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7601,
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
        crate::http_member_test::request(&follower_url, "GET", "/pd/api/v1/hotspot/stores", "").1,
        crate::http_member_test::request(&url, "GET", "/pd/api/v1/hotspot/stores", "").1
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
                    &format!("{follower_url}/pd/api/v1/regions/range-holes"),
                ])
                .output()
                .unwrap();
            if serde_json::from_slice::<Value>(&out.stdout)
                .ok()
                .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        crate::http_member_test::request(&follower_url, "GET", "/pd/api/v1/hotspot/stores", "").1,
        crate::http_member_test::request(&url, "GET", "/pd/api/v1/hotspot/stores", "").1
    );
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            "pd-allow-follower-handle: true",
            "-w",
            "\n%{http_code}",
            &format!("{follower_url}/pd/api/v1/hotspot/stores"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.ends_with("500") && text.contains("PANIC: runtime error:"),
        "{text}"
    );
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            "pd-allow-follower-handle: true",
            &format!("{follower_url}/pd/api/v1/regions/range-holes"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        crate::http_member_test::request(&url, "GET", "/pd/api/v1/regions/range-holes", "").1
    );
    follower.shutdown().await.unwrap();
    let read = Read::prefix(b"/pd/7601/raft/", Consistency::Linearizable);
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
            b"GET /pd/api/v1/hotspot/stores HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
async fn task_060_clock_expiration() {
    use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
    use astersql_pd_statistics::{HotCache, HotTask};
    use astersql_pd_statistics_utils::RWType;
    let peer = Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 2,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    let mut cache = HotCache::new(&tokio::runtime::Handle::current(), 32);
    for rw in [RWType::READ, RWType::WRITE] {
        let interval = rw.report_interval() as u64;
        for _ in 0..6 {
            let task = if rw == RWType::WRITE {
                HotTask::Write {
                    meta: region.meta().clone(),
                    leader_store: 1,
                    interval,
                    counters: [interval * 1048576, interval * 1000, interval * 100],
                }
            } else {
                let mut loads = [0.; 8];
                loads[0] = (interval * 1048576) as f64;
                loads[1] = (interval * 1000) as f64;
                HotTask::read_peer(&region, 1, loads, interval)
            };
            cache.submit(rw, task, 0).unwrap().wait().await.unwrap();
        }
        assert_eq!(cache.get_hot_peer_stats(rw, 3).await.unwrap()[&1].len(), 1);
        let ttl = 3 * interval as i64 * 1_000_000_000;
        for (time, exists) in [(ttl - 1, true), (ttl, false)] {
            cache
                .submit(
                    rw,
                    HotTask::Metrics {
                        active_stores: [1].into_iter().collect(),
                    },
                    time,
                )
                .unwrap()
                .wait()
                .await
                .unwrap();
            assert_eq!(
                cache.get_hot_peer_stat(rw, 2, 1).await.unwrap().is_some(),
                exists
            );
        }
        assert!(
            cache
                .get_hot_peer_stats(rw, 3)
                .await
                .unwrap()
                .values()
                .all(Vec::is_empty)
        );
    }
    cache.shutdown().await.unwrap();
}
