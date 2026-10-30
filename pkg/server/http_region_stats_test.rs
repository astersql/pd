// Copyright 2026 AsterSQL.
use crate::grpc_region_lookup_scenarios_test::header;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn request(url: &str, path: &str) -> (u16, Value) {
    crate::http_member_test::request(url, "GET", &format!("/pd/api/v1{path}"), "")
}
async fn observe(url: &str, id: u64) -> Value {
    let cold = request(url, "/regions/check/hist-size?bound=bad");
    assert_eq!(cold.0, 500, "missing statistics HTTP route");
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    assert!(
        client
            .bootstrap(pdpb::BootstrapRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: 1,
                    version: "8.5.0".into(),
                    ..Default::default()
                }),
                region: Some(metapb::Region {
                    id: 2,
                    peers: vec![peer],
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
    assert_eq!(
        crate::http_member_test::request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"enable-placement-rules":"true"}"#
        )
        .0,
        200
    );
    for (store, engine) in [(2, "tiflash"), (3, "tiflash_compute")] {
        assert!(
            client
                .put_store(pdpb::PutStoreRequest {
                    header: header(id),
                    store: Some(metapb::Store {
                        id: store,
                        address: format!("store-{store}"),
                        version: "8.5.0".into(),
                        labels: vec![metapb::StoreLabel {
                            key: "engine".into(),
                            value: engine.into()
                        }],
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
    let mut out = vec![json!(cold)];
    for path in [
        "/regions/check/hist-size",
        "/regions/check/hist-keys",
        "/stats/region",
        "/stats/region?count=false",
        "/stats/region?use_hot=false",
    ] {
        let r = request(url, path);
        assert_eq!(r.0, 200, "{path}: {r:?}");
        out.push(json!([path, r]));
    }
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    for (i, start, end, size, keys) in [
        (0, b"".as_slice(), b"b".as_slice(), 1, 0),
        (1, b"b", b"d", 3, 1000),
        (2, b"d", b"", 5, 2001),
    ] {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: 100 + i,
                start_key: start.to_vec(),
                end_key: end.to_vec(),
                region_epoch: Some(metapb::RegionEpoch {
                    version: 10,
                    conf_ver: 1,
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            approximate_size: size * 1048576,
            approximate_keys: keys,
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
                    region_id: 102,
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
    for path in [
        "/regions/check/hist-size",
        "/regions/check/hist-size?bound=2",
        "/regions/check/hist-keys",
        "/regions/check/hist-keys?bound=1500",
        "/regions/check/hist-size?bound=-1",
        "/regions/check/hist-size?bound=0",
        "/regions/check/hist-size?bound=9223372036854775807",
        "/regions/check/hist-size?bound=-9223372036854775808",
        "/regions/check/hist-size?bound=&bound=bad",
        "/regions/check/hist-size?bound=%GG",
        "/regions/check/hist-size?bound=1;bad=2",
        "/regions/check/hist-keys?bound=1",
        "/stats/region?start_key=c&end_key=c",
        "/stats/region?start_key=%00&end_key=%FF",
        "/stats/region?count=%GG",
        "/stats/region?start_key=d&end_key=d&use_hot",
        "/stats/region?use_hot&engine=unknown",
        "/regions/check/hist-size?bound=2&bound=3",
        "/regions/check/hist-size?bound=%2B2",
        "/regions/check/hist-size?bound=bad",
        "/regions/check/hist-size?bound=9223372036854775808",
        "/regions/check/hist-size?bound=+1",
        "/stats/region",
        "/stats/region?start_key=c&end_key=e",
        "/stats/region?start_key=d&end_key=d",
        "/stats/region?start_key=z&end_key=a",
        "/stats/region?start_key=b&end_key=d&count=false",
        "/stats/region?start_key=z&end_key=a&count",
        "/stats/region?count&use_hot",
        "/stats/region?use_hot",
        "/stats/region?use_hot&engine=tikv",
        "/stats/region?use_hot&engine=tiflash",
    ] {
        let r = request(url, path);
        assert_eq!(
            r.0,
            if path == "/regions/check/hist-size?bound=bad"
                || path.contains("bound=9223372036854775808")
                || path.contains("=+1")
            {
                400
            } else {
                200
            },
            "{path}: {r:?}"
        );
        out.push(json!([path, r]));
    }
    assert_eq!(
        request(url, "/regions/check/hist-size?bound=2").1,
        json!([{"start":0,"end":1,"count":1},{"start":2,"end":3,"count":1},{"start":4,"end":5,"count":1}])
    );
    assert_eq!(
        request(url, "/stats/region?start_key=c&end_key=e").1["storage_size"],
        8
    );
    // Real heartbeats drive the canonical asynchronous write-hot cache. A full
    // reported minute per sample avoids wall-clock advancement or sleep-based rates.
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    for n in 0..8 {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: 102,
                start_key: b"d".to_vec(),
                region_epoch: Some(metapb::RegionEpoch {
                    version: 20 + n,
                    conf_ver: 1,
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            approximate_size: 5 * 1048576,
            approximate_keys: 2001,
            bytes_written: 60 * 1048576,
            keys_written: 6000,
            interval: Some(pdpb::TimeInterval {
                start_timestamp: 60 * n,
                end_timestamp: 60 * (n + 1),
            }),
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
                    region_id: 102,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if r.region
                .and_then(|r| r.region_epoch)
                .is_some_and(|e| e.version == 27)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    drop(stream);
    let hot = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let r = request(url, "/stats/region?use_hot");
            if r.1["store_write_bytes"]["1"]
                .as_u64()
                .is_some_and(|v| v > 0)
            {
                break r;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("write-hot cache publishes real rates");
    assert_eq!(hot.1["store_write_bytes"]["1"], 1048576);
    assert_eq!(hot.1["store_write_keys"]["1"], 100);
    out.push(json!(["hot-write", hot]));
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
                cluster_id: 758,
                member_id: 1,
                name: "stats".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let r = observe(&url, 758).await;
    let read = Read::prefix(b"/pd/758/raft/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in [
        "/regions/check/hist-size",
        "/regions/check/hist-keys",
        "/stats/region",
        "/stats/region?use_hot",
    ] {
        assert_eq!(request(&url, path).0, 200);
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
    r
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_058_behavior() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_058_differential() {
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
async fn task_058_cancel_and_follower() {
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
                cluster_id: 7571,
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7571).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7571,
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
        request(&follower_url, "/regions/check/hist-size").1,
        request(&url, "/regions/check/hist-size").1
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
                    &format!("{follower_url}/pd/api/v1/regions/check/hist-size"),
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
        request(&follower_url, "/stats/region").1,
        request(&url, "/stats/region").1
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
            &format!("{follower_url}/pd/api/v1/stats/region"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.ends_with("500") && text.contains("ErrNotBootstrapped"),
        "{text}"
    );
    follower.shutdown().await.unwrap();
    let read = Read::prefix(b"/pd/7571/raft/", Consistency::Linearizable);
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
            b"GET /pd/api/v1/regions/check/hist-size HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
async fn task_058_histogram_empty_signed_overflow_cancel() {
    use astersql_pd_core::{RegionInfo, RegionMeta};
    let cancel = CancellationToken::new();
    assert_eq!(
        crate::http_region_stats::histogram(&[], true, 1000, &cancel)
            .await
            .unwrap(),
        Vec::<Value>::new()
    );
    let regions = [i64::MIN, -1001, -1, 0, 1000, i64::MAX]
        .into_iter()
        .enumerate()
        .map(|(i, keys)| {
            let mut r = RegionInfo::new(
                RegionMeta {
                    id: i as u64 + 1,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            );
            r.stats_mut().approximate_keys = keys;
            Arc::new(r)
        })
        .collect::<Vec<_>>();
    let rows = crate::http_region_stats::histogram(&regions, true, 1000, &cancel)
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|v| v["count"].as_i64().unwrap())
            .sum::<i64>(),
        6
    );
    assert!(rows.contains(&json!({"start":0,"end":999,"count":2})));
    assert!(rows.contains(&json!({"start":-1000,"end":-1,"count":1})));
    assert!(rows.contains(
        &json!({"start":9223372036854775000i64,"end":-9223372036854775617i64,"count":1})
    ));
    cancel.cancel();
    assert_eq!(
        crate::http_region_stats::histogram(&regions, true, 1000, &cancel)
            .await
            .unwrap_err(),
        "context canceled"
    );
}
