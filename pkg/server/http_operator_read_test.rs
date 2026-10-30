// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::pd_client::PdClient;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn request(url: &str, path: &str) -> (u16, Value) {
    crate::http_member_test::request(url, "GET", &format!("/pd/api/v1/operators{path}"), "")
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_062_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 762,
                member_id: 1,
                name: "operators".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let result = request(&url, "");
    assert_eq!(
        result,
        (
            500,
            json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            )
        ),
        "operator query HTTP missing"
    );
    let observations = observe(&url, 762).await;
    println!("{observations}");
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = Read::prefix(b"/pd/762/raft/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap().entries;
    for path in ["", "?kind=leader&object=false", "/records", "/999"] {
        request(&url, path);
    }
    assert_eq!(before, store.read(read).await.unwrap().entries);
    node.shutdown().await.unwrap();
}

async fn observe(url: &str, id: u64) -> Value {
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let mut observations = Vec::new();
    for path in ["", "/999", "/records"] {
        let r = request(url, path);
        assert_eq!(
            r,
            (
                500,
                json!(
                    "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
                )
            )
        );
        observations.push(json!(["cold", path, r]));
    }
    for path in ["", "/records", "/999"] {
        for method in ["HEAD", "PUT", "OPTIONS"] {
            let out = std::process::Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "5",
                    "-H",
                    "Connection: close",
                    "-X",
                    method,
                    "-w",
                    "\n%{http_code}",
                    &format!("{url}/pd/api/v1/operators{path}"),
                ])
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
            let text = String::from_utf8(out.stdout).unwrap();
            assert_eq!(
                text,
                if method == "OPTIONS" {
                    "\n200"
                } else {
                    "\n405"
                }
            );
            observations.push(json!([method, path, text]));
        }
    }

    for path in [
        "?kind=unknown",
        "?kind=&object=false",
        "/bad",
        "/184467440737095516160x",
        "/18446744073709551615x",
        "/records?from=184467440737095516160x",
        "/records?from=9223372036854775808x",
        "/records?from=%2B18446744073709551616",
        "/+1",
        "/18446744073709551616",
        "/records?from=bad",
        "/records?from=9223372036854775808",
        "/records?from=+1",
    ] {
        observations.push(json!([path, request(url, path)]));
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
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
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
    assert!(boot.header.unwrap().error.is_none());
    for path in [
        "",
        "?object=false",
        "?kind=leader",
        "?kind=waiting",
        "/999",
        "/records",
        "/records?from=",
        "/records?from=-9223372036854775808",
        "/records?from=9223372036854775807",
        "/records?from=%2B0",
    ] {
        let r = request(url, path);
        if path.starts_with("/records") || path == "/999" {
            assert_eq!(
                r,
                (
                    500,
                    json!("[PD:operator:ErrOperatorNotFound]operator not found")
                )
            )
        } else {
            assert_eq!(
                r,
                (
                    200,
                    if path.contains("kind=") {
                        Value::Null
                    } else {
                        json!([])
                    }
                )
            );
        }
        observations.push(json!([path, r]));
    }
    for store_id in 1..=3 {
        client
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
            .unwrap();
    }
    let peers: Vec<_> = (1..=3)
        .map(|store_id| metapb::Peer {
            id: 300 + store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let regions: Vec<_> = (100..=102)
        .map(|region_id| metapb::Region {
            id: region_id,
            start_key: vec![region_id as u8],
            end_key: vec![region_id as u8 + 1],
            peers: peers.clone(),
            region_epoch: Some(metapb::RegionEpoch {
                version: 3,
                conf_ver: 1,
            }),
            ..Default::default()
        })
        .collect();
    for region in &regions {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            region: Some(region.clone()),
            leader: Some(peers[0].clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let mut heartbeat_client = client.clone();
    let pending_stream = tokio::spawn(async move {
        heartbeat_client
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner()
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let mut ready = true;
            for region in &regions {
                ready &= client
                    .get_region_by_id(pdpb::GetRegionByIdRequest {
                        header: crate::grpc_region_lookup_scenarios_test::header(id),
                        region_id: region.id,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner()
                    .region
                    .is_some();
            }
            if ready {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            regions_id: vec![100, 101, 102],
            group: "task062".into(),
            skip_store_limit: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut stream = tokio::time::timeout(std::time::Duration::from_secs(20), pending_stream)
        .await
        .unwrap()
        .unwrap();
    let objects = request(url, "?object=false");
    assert_eq!(objects.0, 200);
    let objects = objects.1.as_array().unwrap();
    assert_eq!(objects.len(), 2);
    let strings = request(url, "").1;
    let strings = strings.as_array().unwrap();
    assert_eq!(strings.len(), 2);
    let time=regex::Regex::new(r"(?:createAt|startAt):[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9:.]+ [+-][0-9]{4} [A-Za-z0-9:+-]+ m=[+-][0-9]+\.[0-9]{9}").unwrap();
    let mut commands = std::collections::BTreeMap::new();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while commands.len() < 2 {
            let r = stream.message().await.unwrap().unwrap();
            if let Some(t) = r.transfer_leader {
                commands.insert(r.region_id, t.peer.unwrap());
            }
        }
    })
    .await
    .unwrap();
    for op in objects {
        assert_eq!(op["desc"], "scatter-region");
        assert_eq!(op["region_epoch"], json!({"version":3,"conf_ver":1}));
        assert_eq!(op["status"], 1);
        assert_eq!(op["kind"], 257);
        assert_eq!(op["timeout"], "1m0s");
        let region = op["region_id"].as_u64().unwrap();
        let target = commands.get(&region).unwrap().store_id;
        let status = request(url, &format!("/{region}"));
        assert_eq!(status.0, 200);
        let text = status.1.as_str().unwrap();
        assert!(
            text.starts_with("status: RUNNING, operator: scatter-region {"),
            "{text}"
        );
        assert!(text.contains(&format!("region:{region}(3, 1)")), "{text}");
        assert!(
            text.contains(&format!("transfer leader from store 1 to store {target}")),
            "{text}"
        );
        assert_eq!(time.find_iter(text).count(), 2, "{text}");
        let date=regex::Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9:.]+ [+-][0-9]{4} [A-Za-z0-9:+-]+ m=[+-][0-9]+\.[0-9]{9}").unwrap();
        assert_eq!(
            date.replace_all(text, "<time>"),
            format!(
                "status: RUNNING, operator: scatter-region {{transfer leader: store 1 to {target}}} (kind:admin,leader, region:{region}(3, 1), createAt:<time>, startAt:<time>, currentStep:0, size:1, steps:[0:{{transfer leader from store 1 to store {target}}}], timeout:[1m0s])"
            )
        );
        assert!(strings.iter().any(|s| text.ends_with(s.as_str().unwrap())));
    }
    for (path, count) in [
        ("?kind=leader", 2),
        ("?kind=leader&kind=leader", 4),
        ("?kind=leader&kind=unknown", 4),
        ("?kind=unknown&kind=leader", 2),
    ] {
        assert_eq!(request(url, path).1.as_array().unwrap().len(), count);
    }
    for (region, leader) in &commands {
        tx.send(pdpb::RegionHeartbeatRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            region: regions.iter().find(|r| r.id == *region).cloned(),
            leader: Some(leader.clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if commands.keys().all(|r| {
                request(url, &format!("/{r}"))
                    .1
                    .as_str()
                    .is_some_and(|s| s.starts_with("status: SUCCESS,"))
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let records = request(url, "/records");
    assert_eq!(records.0, 200);
    let records = records.1.as_array().unwrap();
    assert_eq!(records.len(), 2);
    for r in records {
        let text = r.as_str().unwrap();
        assert!(text.contains(" finished (finishAt:"), "{text}");
        assert!(text.contains(", duration:"));
        assert_eq!(time.find_iter(text).count(), 2);
    }
    let mut second: Vec<_> = request(url, "/records")
        .1
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    second.sort();
    let mut first: Vec<_> = records
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    first.sort();
    assert_eq!(
        first, second,
        "finished timestamps/durations must remain stable"
    );
    assert_eq!(request(url, "").1, json!([]));
    for (from, count) in [
        ("", 2),
        ("0", 2),
        ("-9223372036854775808", 2),
        ("9223372036854775807", 2),
        ("9223371974719179007", 0),
        ("9223371974719179008", 2),
        ("9999999999", 0),
    ] {
        let r = request(url, &format!("/records?from={from}"));
        if count == 0 {
            assert_eq!(
                r,
                (
                    500,
                    json!("[PD:operator:ErrOperatorNotFound]operator not found")
                )
            )
        } else {
            assert_eq!(r.0, 200);
            assert_eq!(r.1.as_array().unwrap().len(), count)
        }
        observations.push(json!([from, r.0, count]));
    }

    // A real epoch change cancels a new admitted operator and retains CANCEL.
    for region in &regions {
        let mut region = region.clone();
        region.region_epoch.as_mut().unwrap().version = 4;
        tx.send(pdpb::RegionHeartbeatRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            region: Some(region),
            leader: Some(peers[0].clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let mut ready = true;
            for region in &regions {
                let r = client
                    .get_region_by_id(pdpb::GetRegionByIdRequest {
                        header: crate::grpc_region_lookup_scenarios_test::header(id),
                        region_id: region.id,
                        ..Default::default()
                    })
                    .await
                    .unwrap()
                    .into_inner();
                ready &= r.leader == Some(peers[0].clone())
                    && r.region
                        .and_then(|r| r.region_epoch)
                        .is_some_and(|e| e.version == 4);
            }
            if ready {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    client
        .scatter_region(pdpb::ScatterRegionRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            regions_id: vec![100, 101, 102],
            group: "task062cancel".into(),
            skip_store_limit: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let canceled = request(url, "?object").1;
    let canceled = canceled.as_array().unwrap();
    assert_eq!(canceled.len(), 2);
    for op in canceled {
        let rid = op["region_id"].as_u64().unwrap();
        assert!(
            request(url, &format!("/{rid}"))
                .1
                .as_str()
                .unwrap()
                .starts_with("status: RUNNING,")
        );
        let mut region = regions.iter().find(|r| r.id == rid).unwrap().clone();
        region.region_epoch = Some(metapb::RegionEpoch {
            version: 4,
            conf_ver: 2,
        });
        tx.send(pdpb::RegionHeartbeatRequest {
            header: crate::grpc_region_lookup_scenarios_test::header(id),
            region: Some(region),
            leader: Some(peers[0].clone()),
            approximate_size: 1048576,
            ..Default::default()
        })
        .await
        .unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if canceled.iter().all(|op| {
                request(url, &format!("/{}", op["region_id"].as_u64().unwrap()))
                    .1
                    .as_str()
                    .is_some_and(|s| s.starts_with("status: CANCEL, operator: scatter-region"))
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let cancel_records = request(url, "/records");
    assert_eq!(cancel_records.0, 200);
    for op in canceled {
        assert!(
            cancel_records
                .1
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r.as_str().unwrap().contains(&format!(
                    "region:{}(4, 1)",
                    op["region_id"].as_u64().unwrap()
                )))
        );
    }
    assert_eq!(request(url, "").1, json!([]));
    observations.push(json!({"canceled":canceled.len(),"retained_status":"CANCEL"}));
    println!("operator objects: {objects:?}; strings: {strings:?}; records: {records:?}");
    observations.push(json!({"running":objects.len(),"commands":commands.len(),"records":records.len(),"statuses":["RUNNING","SUCCESS"],"text_contract":true}));
    drop(tx);
    drop(stream);
    json!(observations)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_062_differential() {
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(go.url.clone())
        .await
        .unwrap();
    let id = client
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let go_observations = observe(&go.url, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let go_restart = restart_observe(&go.url);
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7621,
                member_id: 1,
                name: "operators".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&url, 7621).await;
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    let restarted_url = url.clone();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                NodeConfig {
                    cluster_id: 7621,
                    member_id: 1,
                    name: "operators".into(),
                    client_url: restarted_url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    let rust_restart = restart_observe(&restarted_url);
    restarted.shutdown().await.unwrap();
    assert_eq!(go_restart, rust_restart);
    println!("restart Go/Rust: {go_restart}; Go: {go_observations}\nRust: {rust}");
    assert_eq!(go_observations, rust);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_062_cancel_and_follower() {
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
                cluster_id: 7622,
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
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7622).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7622,
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
    assert_eq!(request(&follower_url, "/999"), request(&url, "/999"));
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            "pd-allow-follower-handle: true",
            "-w",
            "\n%{http_code}",
            &format!("{follower_url}/pd/api/v1/operators/999"),
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
    let read = Read::prefix(b"/pd/7622/raft/", Consistency::Linearizable);
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
            b"GET /pd/api/v1/operators HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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

fn restart_observe(url: &str) -> Value {
    let list = request(url, "");
    assert_eq!(list, (200, json!([])));
    let records = request(url, "/records");
    assert_eq!(
        records,
        (
            500,
            json!("[PD:operator:ErrOperatorNotFound]operator not found")
        )
    );
    json!([list, records])
}
