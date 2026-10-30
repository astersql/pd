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
const CHECKS: &[&str] = &[
    "miss-peer",
    "extra-peer",
    "pending-peer",
    "down-peer",
    "learner-peer",
    "offline-peer",
    "empty-region",
    "oversized-region",
    "undersized-region",
];
fn request(url: &str, path: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "8",
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
        if code == 404 {
            "text/plain; charset=utf-8"
        } else if code == 200 && path.contains("/check/") {
            "application/octet-stream"
        } else {
            "application/json; charset=UTF-8"
        },
        "{path}: {body}"
    );
    (
        code,
        serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
    )
}
async fn observe(url: &str, id: u64) -> Value {
    let cold = request(url, "/regions/check/miss-peer");
    assert_eq!(cold.0, 500, "missing region check HTTP route");
    let mut out = vec![json!(cold)];
    for path in [
        "/regions/replicated",
        "/regions/replicated?startKey=",
        "/regions/replicated?endKey=",
    ] {
        let r = request(url, path);
        assert_eq!(r, (404, json!("404 page not found\n")));
        out.push(json!([path, r]));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
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
                peers: vec![peer.clone()],
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    for store in 2..=4 {
        client
            .put_store(pdpb::PutStoreRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: store,
                    address: format!("store-{store}"),
                    version: "8.5.0".into(),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    // One Region avoids map-order normalization of Go's anomaly sets.
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header(id),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![peer.clone()],
            region_epoch: Some(metapb::RegionEpoch {
                version: 1,
                conf_ver: 1,
            }),
            ..Default::default()
        }),
        leader: Some(peer.clone()),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        client.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await
    .unwrap()
    .unwrap();
    for (version, count, learner, pending, down, size, keys) in [
        (2, 1, false, false, false, 1, 0),
        (3, 4, false, false, false, 200, 2000000),
        (4, 3, true, true, true, 2, 100),
        (5, 3, false, false, false, 96, 100000),
    ] {
        let mut peers: Vec<_> = (1..=count)
            .map(|store| metapb::Peer {
                id: store + 2,
                store_id: store,
                ..Default::default()
            })
            .collect();
        if learner {
            peers.last_mut().unwrap().role = metapb::PeerRole::Learner as i32;
        }
        tx.send(pdpb::RegionHeartbeatRequest {
            header: header(id),
            region: Some(metapb::Region {
                id: 2,
                peers: peers.clone(),
                region_epoch: Some(metapb::RegionEpoch {
                    version,
                    conf_ver: version,
                }),
                ..Default::default()
            }),
            leader: Some(peer.clone()),
            pending_peers: if pending {
                vec![peers[2].clone()]
            } else {
                vec![]
            },
            down_peers: if down {
                vec![pdpb::PeerStats {
                    peer: Some(peers[1].clone()),
                    down_seconds: 17,
                }]
            } else {
                vec![]
            },
            approximate_size: size * 1048576,
            approximate_keys: keys,
            ..Default::default()
        })
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let r = client
                    .get_region_by_id(pdpb::GetRegionByIdRequest {
                        header: header(id),
                        region_id: 2,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                let classified = request(
                    url,
                    if count == 1 {
                        "/regions/check/miss-peer"
                    } else if count == 4 {
                        "/regions/check/extra-peer"
                    } else if down {
                        "/regions/check/down-peer"
                    } else {
                        "/regions/check/down-peer"
                    },
                );
                if r.region
                    .as_ref()
                    .and_then(|r| r.region_epoch.as_ref())
                    .is_some_and(|e| e.version == version)
                    && classified.1["count"] == json!(if version == 5 { 0 } else { 1 })
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        for kind in CHECKS {
            let r = request(url, &format!("/regions/check/{kind}"));
            assert_eq!(r.0, 200);
            let expected = match version {
                2 => ["miss-peer", "empty-region", "undersized-region"].contains(kind),
                3 => ["extra-peer", "oversized-region"].contains(kind),
                4 => [
                    "pending-peer",
                    "down-peer",
                    "learner-peer",
                    "undersized-region",
                ]
                .contains(kind),
                _ => false,
            };
            assert_eq!(
                r.1["count"],
                json!(u64::from(expected)),
                "version={version} {kind}"
            );
            assert_eq!(
                r.1["count"].as_u64().unwrap(),
                r.1["regions"].as_array().unwrap().len() as u64
            );
            out.push(json!([version, kind, r]));
        }
        for path in [
            "/regions/range-holes",
            "/regions/replicated?startKey=&endKey=",
            "/regions/replicated?startKey=zz&endKey=",
            "/regions/replicated?startKey=%FF&endKey=",
            "/regions/replicated?startKey=%27&endKey=",
            "/regions/replicated?startKey=&endKey=1",
            "/regions/replicated?startKey=1&endKey=",
            "/regions/replicated?startKey=61&endKey=62",
            "/regions/replicated?startKey=62&endKey=61",
        ] {
            let r = request(url, path);
            out.push(json!([version, path, r]));
        }
    }
    drop(tx);
    drop(stream);
    // Removing a Store is reflected by a subsequent observation, not recomputed by HTTP.
    assert_eq!(
        crate::http_member_test::request(url, "DELETE", "/pd/api/v1/store/3", "").0,
        200
    );
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header(id),
        region: Some(metapb::Region {
            id: 2,
            start_key: b"a".to_vec(),
            end_key: b"z".to_vec(),
            peers: (1..=3)
                .map(|store| metapb::Peer {
                    id: store + 2,
                    store_id: store,
                    ..Default::default()
                })
                .collect(),
            region_epoch: Some(metapb::RegionEpoch {
                version: 10,
                conf_ver: 10,
            }),
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        client.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let r = request(url, "/regions/check/offline-peer");
            if r.1["count"] == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for path in ["/regions/check/offline-peer", "/regions/range-holes"] {
        out.push(json!([path, request(url, path)]));
    }
    assert_eq!(
        crate::http_member_test::request(
            url,
            "POST",
            "/pd/api/v1/config/replicate",
            r#"{"max-replicas":5}"#
        )
        .0,
        200
    );
    // Scatter routes an incomplete Region to the checker retry queue.
    #[allow(deprecated)]
    let rejected = client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: header(id),
            region_id: 2,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(rejected.code(), tonic::Code::Unknown);
    assert_eq!(rejected.message(), "region 2 is not fully replicated");
    // Scatter's retry queue differs from rule-pending membership. Merely
    // accelerating an incomplete Region must not report PENDING.
    let state = request(url, "/regions/replicated?startKey=&endKey=");
    assert_eq!(state, (200, json!("INPROGRESS")));
    out.push(json!(["scatter-retry-is-not-rule-pending", state]));
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
                cluster_id: 759,
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
    let r = observe(&url, 759).await;
    let read = Read::prefix(b"/pd/759/raft/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in [
        "/regions/check/miss-peer",
        "/regions/range-holes",
        "/regions/replicated?startKey=&endKey=",
    ] {
        assert_eq!(request(&url, path).0, 200);
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
    r
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_059_behavior() {
    rust_observe().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_059_differential() {
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
async fn task_059_cancel_and_follower() {
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
                cluster_id: 7591,
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7591).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7591,
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
        request(&follower_url, "/regions/check/miss-peer").1,
        request(&url, "/regions/check/miss-peer").1
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
        request(&follower_url, "/regions/check/miss-peer").1,
        request(&url, "/regions/check/miss-peer").1
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
            &format!("{follower_url}/pd/api/v1/regions/check/miss-peer"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.ends_with("500") && text.contains("ErrNotBootstrapped"),
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
        request(&url, "/regions/range-holes").1
    );
    follower.shutdown().await.unwrap();
    let read = Read::prefix(b"/pd/7591/raft/", Consistency::Linearizable);
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
            b"GET /pd/api/v1/regions/check/miss-peer HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
