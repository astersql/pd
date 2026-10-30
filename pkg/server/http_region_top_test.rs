// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_057_behavior() {
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
    let response =
        crate::http_member_test::request(&url, "GET", "/pd/api/v1/regions/writeflow", "");
    node.shutdown().await.unwrap();
    assert_eq!(response.0, 500, "unbootstrapped region scan: {response:?}");
}

use crate::grpc_region_lookup_scenarios_test::header;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
const DIMS: [&str; 9] = [
    "writeflow",
    "writequery",
    "readflow",
    "readquery",
    "confver",
    "version",
    "size",
    "keys",
    "cpu",
];
fn request(url: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    let code: u16 = code.parse().unwrap();
    assert_eq!(
        content,
        if code == 200 {
            "application/octet-stream"
        } else {
            "application/json; charset=UTF-8"
        },
        "{path}: {text}"
    );
    (code, serde_json::from_str(body).unwrap())
}
fn metric(r: &Value, dim: &str) -> i128 {
    let v = match dim {
        "writeflow" => &r["written_bytes"],
        "readflow" => &r["read_bytes"],
        "writequery" | "readquery" => {
            // Query counters are not exposed in Region JSON. Recover the input
            // fixture's rank by identity, independently of output position.
            let factor = if dim == "writequery" { 19 } else { 23 };
            let rank = (r["id"].as_u64().unwrap() - 1000) * factor % 24;
            let rank = if r["epoch"]["version"].as_u64().unwrap() >= 100 {
                rank / 2 + 1
            } else {
                rank + 1
            };
            return ((1u64 << 53) + rank) as i128;
        }
        "confver" => &r["epoch"]["conf_ver"],
        "version" => &r["epoch"]["version"],
        "size" => &r["approximate_size"],
        "keys" => &r["approximate_keys"],
        "cpu" => &r["cpu_usage"],
        _ => unreachable!(),
    };
    v.as_i64()
        .map(i128::from)
        .unwrap_or_else(|| v.as_u64().unwrap() as i128)
}
async fn seed(client: &mut PdClient<tonic::transport::Channel>, id: u64, ties: bool) {
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    for i in 0..24u64 {
        let rank = |factor: u64| {
            let rank = (i * factor) % 24;
            if ties { rank / 2 + 1 } else { rank + 1 }
        };
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: 1000 + i,
                start_key: if i == 0 {
                    vec![]
                } else {
                    format!("{i:04}").into_bytes()
                },
                end_key: if i == 23 {
                    vec![]
                } else {
                    format!("{:04}", i + 1).into_bytes()
                },
                region_epoch: Some(metapb::RegionEpoch {
                    version: rank(7) + if ties { 100 } else { 10 },
                    conf_ver: rank(5) + if ties { 100 } else { 0 },
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            bytes_written: rank(5) * 1024,
            bytes_read: rank(7) * 1024,
            approximate_size: rank(11) * 1048576,
            approximate_keys: if i == 0 { u64::MAX } else { rank(13) },
            cpu_usage: (1u64 << 53) + rank(17),
            query_stats: Some(pdpb::QueryStats {
                put: (1u64 << 53) + rank(19),
                get: (1u64 << 53) + rank(23),
                ..Default::default()
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
                    region_id: 1023,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if r.region
                .as_ref()
                .and_then(|r| r.region_epoch.as_ref())
                .is_some_and(|e| e.version == if ties { 109 } else { 28 })
                && r.leader == Some(peer.clone())
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
}
async fn observe(url: &str, id: u64) -> Value {
    let cold = request(url, "/regions/cpu?limit=bad");
    assert_eq!(cold.0, 500);
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    seed(&mut client, id, false).await;
    let bad_store = request(url, "/regions/store/cpu");
    assert_eq!(
        bad_store.0, 400,
        "store ID must not be dispatched as a top dimension"
    );
    let mut results = vec![json!(cold), json!(bad_store)];
    for dim in DIMS {
        let all = request(url, &format!("/regions/{dim}?limit=10241"));
        assert_eq!(all.0, 200);
        assert_eq!(all.1["count"], 24);
        let rows = all.1["regions"].as_array().unwrap();
        assert!(
            rows.windows(2)
                .all(|w| metric(&w[0], dim) > metric(&w[1], dim)),
            "{dim}: {rows:?}"
        );
        if dim == "keys" {
            assert_eq!(rows.last().unwrap()["approximate_keys"], -1);
        }
        for (suffix, n) in [
            ("", 16),
            ("?limit=", 16),
            ("?limit=1", 1),
            ("?limit=23", 23),
            ("?limit=0", 0),
            ("?limit=-1", 0),
            ("?limit=-9223372036854775808", 0),
            ("?limit=9223372036854775807", 24),
            ("?limit=%2B2", 2),
            ("?limit=2&limit=3", 2),
            ("?limit=%GG", 16),
            ("?limit=1;ignored=2", 16),
        ] {
            let r = request(url, &format!("/regions/{dim}{suffix}"));
            assert_eq!(r.0, 200);
            assert_eq!(r.1["count"], n);
            assert_eq!(r.1["regions"], json!(&rows[..n]));
            results.push(json!([dim, suffix, r]));
        }
        for suffix in [
            "?limit=a",
            "?limit=+1",
            "?limit=9223372036854775808",
            "?limit=-9223372036854775809",
            "?limit=1.5",
        ] {
            let r = request(url, &format!("/regions/{dim}{suffix}"));
            assert_eq!(r.0, 400);
            results.push(json!([dim, suffix, r]));
        }
        results.push(json!([dim, "all", all]));
    }
    seed(&mut client, id, true).await;
    for dim in DIMS {
        let mut all = request(url, &format!("/regions/{dim}?limit=100"));
        let rows = all.1["regions"].as_array_mut().unwrap();
        assert!(
            rows.windows(2)
                .all(|w| metric(&w[0], dim) >= metric(&w[1], dim))
        );
        // Go iterates a map before heap selection. Only equal-value groups have
        // unspecified identity/order; do not normalize unequal rankings.
        rows.sort_by(|a, b| {
            metric(b, dim)
                .cmp(&metric(a, dim))
                .then_with(|| a["id"].as_u64().cmp(&b["id"].as_u64()))
        });
        let cut = request(url, &format!("/regions/{dim}?limit=3"));
        let cut_rows = cut.1["regions"].as_array().unwrap();
        assert_eq!(cut_rows.len(), 3);
        for (a, b) in cut_rows.iter().zip(rows.iter()) {
            assert_eq!(metric(a, dim), metric(b, dim));
            assert!(rows.contains(a));
        }
        let mut ids = cut_rows
            .iter()
            .map(|r| r["id"].as_u64().unwrap())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3);
        results.push(json!([dim, "ties", all]));
    }
    json!(results)
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
                cluster_id: 757,
                member_id: 1,
                name: "top".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = observe(&url, 757).await;
    let read = Read::prefix(b"/pd/757/raft/r/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for dim in DIMS {
        assert_eq!(request(&url, &format!("/regions/{dim}")).0, 200);
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_057_rankings_limits_and_ties() {
    rust_observe().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_057_differential() {
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
async fn task_057_cancel_and_follower() {
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
        request(&follower_url, "/regions/writeflow?limit=1").1,
        request(&url, "/regions/writeflow?limit=1").1
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
                    &format!("{follower_url}/pd/api/v1/regions/writeflow?limit=1"),
                ])
                .output()
                .unwrap();
            if serde_json::from_slice::<Value>(&out.stdout)
                .ok()
                .is_some_and(|v| v["count"] == 1)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
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
            b"GET /pd/api/v1/regions/writeflow?limit=1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
async fn task_057_heap_ties_signed_and_cancel() {
    use astersql_pd_core::{RegionInfo, RegionMeta};
    let regions = (1..=5)
        .map(|id| {
            Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            ))
        })
        .collect::<Vec<_>>();
    let cancel = CancellationToken::new();
    let selected = crate::http_region_top::select(regions.clone(), "cpu", 3, &cancel)
        .await
        .unwrap();
    assert_eq!(
        selected.iter().map(|r| r.meta().id).collect::<Vec<_>>(),
        vec![2, 3, 1]
    );
    assert!(
        crate::http_region_top::select(vec![], "cpu", 16, &cancel)
            .await
            .unwrap()
            .is_empty()
    );
    cancel.cancel();
    assert_eq!(
        crate::http_region_top::select(regions, "cpu", 3, &cancel)
            .await
            .unwrap_err(),
        "context canceled"
    );
    let regions = [i64::MIN, -1, 0, i64::MAX]
        .into_iter()
        .enumerate()
        .map(|(i, n)| {
            let mut r = RegionInfo::new(
                RegionMeta {
                    id: i as u64,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            );
            r.stats_mut().approximate_keys = n;
            Arc::new(r)
        })
        .collect();
    let result = crate::http_region_top::select(regions, "keys", 4, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        result
            .iter()
            .map(|r| r.stats().approximate_keys)
            .collect::<Vec<_>>(),
        [i64::MAX, 0, -1, i64::MIN]
    );
}

#[tokio::test]
async fn task_057_limit_cap() {
    use astersql_pd_core::{RegionInfo, RegionMeta};
    let regions = (0..10245)
        .map(|id| {
            let mut region = RegionInfo::new(
                RegionMeta {
                    id,
                    ..Default::default()
                },
                None,
                vec![],
                vec![],
            );
            region.set_response_cpu_usage(id);
            Arc::new(region)
        })
        .collect();
    let result =
        crate::http_region_top::select(regions, "cpu", i64::MAX, &CancellationToken::new())
            .await
            .unwrap();
    assert_eq!(result.len(), 10240);
    assert_eq!(result.first().unwrap().meta().id, 10244);
    assert_eq!(result.last().unwrap().meta().id, 5);
}
