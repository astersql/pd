// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn request(url: &str, method: &str, path: &str, body: Value) -> (u16, Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            &body.to_string(),
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/operators{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();

    let (body, code) = text.rsplit_once('\n').unwrap();
    {
        let code: u16 = code.parse().unwrap();
        if content == "text/plain; charset=utf-8" {
            assert_eq!(code, 500);
            assert!(body.contains("PANIC") || body.contains("panic"), "{body}");
            println!("Go recovery response: {body}");
        } else {
            assert_eq!(content, "application/json; charset=UTF-8");
        }
        (
            code,
            serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
        )
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_063_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(crate::http_store_state_test::StateStore {
        inner: Arc::new(fixture.store().await),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 763,
                member_id: 1,
                name: "operator-write".into(),
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
        request(&url, "POST", "", json!({})),
        (400, json!("missing operator name")),
        "write HTTP must dispatch input validation"
    );
    println!("{}", observe(&url, 763, Some(&store)).await);
    node.shutdown().await.unwrap();
}

async fn observe(
    url: &str,
    id: u64,
    fault: Option<&Arc<crate::http_store_state_test::StateStore>>,
) -> Value {
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let mut observations = Vec::new();
    // The pinned Go server's typed-nil bootstrap crash has an unstable stack.
    // Preserve/print its raw response and compare only the 500 status for those
    // two requests; Rust intentionally returns the stable bootstrap JSON error.
    // No other status/body/object field is removed from differential comparison.
    let invalid = vec![
        json!(null),
        json!({}),
        json!({"name":1}),
        json!({"name":"unknown"}),
        json!({"name":"transfer-region","region_id":100,"to_store_ids":[]}),
        json!({"name":"split-region","region_id":100,"policy":"SCAN","keys":[1]}),
    ];
    for input in invalid {
        let r = request(url, "POST", "", input.clone());
        assert_eq!(r.0, 400, "{input}: {r:?}");
        observations.push(json!([input, r]));
    }
    for name in [
        "transfer-leader",
        "transfer-region",
        "transfer-peer",
        "add-peer",
        "add-learner",
        "remove-peer",
        "merge-region",
        "split-region",
        "scatter-region",
        "scatter-regions",
    ] {
        let input = json!({"name":name});
        let r = request(url, "POST", "", input.clone());
        assert_eq!(r.0, 400, "{input}: {r:?}");
        observations.push(json!([input, r]));
        let input = json!({"name":name,"region_id":100,"source_region_id":100});
        let r = request(url, "POST", "", input.clone());
        assert_eq!(
            r.0,
            if name == "scatter-region" { 500 } else { 400 },
            "{input}: {r:?}"
        );
        if name == "scatter-region" {
            println!("cold scatter raw: {r:?}");
            observations.push(json!([input, r.0]));
        } else {
            observations.push(json!([input, r]));
        }
    }
    for (method, path, input) in [
        ("DELETE", "", Value::Null),
        ("DELETE", "/100", Value::Null),
        ("DELETE", "/bad", Value::Null),
        ("DELETE", "/records", Value::Null),
        ("DELETE", "/18446744073709551616", Value::Null),
        (
            "POST",
            "",
            json!({"name":"transfer-leader","region_id":100,"to_store_id":2}),
        ),
        (
            "POST",
            "",
            json!({"name":"scatter-regions","region_ids":[]}),
        ),
    ] {
        let r = request(url, method, path, input.clone());
        if input["name"] == "transfer-leader" {
            assert_eq!(r.0, 500);
            println!("cold leader raw: {r:?}");
            observations.push(json!(["cold", method, path, input, r.0]));
        } else {
            observations.push(json!(["cold", method, path, input, r]));
        }
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
    for store_id in 1..=4 {
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
    for store_id in 1..=4 {
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
    let mut pending_stream = tokio::spawn(async move {
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

    let cases = vec![
        json!({"name":"transfer-leader","region_id":100,"to_store_id":2}),
        json!({"name":"transfer-region","region_id":100,"to_store_ids":[2,3,4],"peer_roles":["leader","voter","voter"]}),
        json!({"name":"transfer-peer","region_id":100,"from_store_id":3,"to_store_id":4}),
        json!({"name":"add-peer","region_id":100,"store_id":4}),
        json!({"name":"add-learner","region_id":100,"store_id":4}),
        json!({"name":"remove-peer","region_id":100,"store_id":3}),
        json!({"name":"merge-region","source_region_id":100,"target_region_id":101}),
        json!({"name":"split-region","region_id":100,"policy":"USEKEY","keys":["64"]}),
        json!({"name":"scatter-region","region_id":100,"group":"task063"}),
        json!({"name":"scatter-regions","region_ids":[100,101,102],"group":"task063","retry_limit":0}),
    ];
    // Keep an independent region's operator alive through validation/conflicts.
    use astersql_pd_storage::{Consistency, Read};
    let raft_before = if let Some(store) = fault {
        Some(
            store
                .inner
                .read(Read::prefix(
                    format!("/pd/{id}/raft/").as_bytes(),
                    Consistency::Linearizable,
                ))
                .await
                .unwrap()
                .entries,
        )
    } else {
        None
    };
    let sentinel = json!({"name":"transfer-leader","region_id":102,"to_store_id":3});
    let r = request(url, "POST", "", sentinel);
    assert_eq!(r.0, 200, "sentinel {r:?}");
    let sentinel_before = request(url, "GET", "/102", Value::Null);
    for input in &cases {
        // Every command conflicts against the independent running sentinel.
        // Compare the entire active set and retained status, including batches.
        let mut conflict = input.clone();
        match conflict["name"].as_str().unwrap() {
            "merge-region" => {
                conflict["source_region_id"] = json!(102);
                conflict["target_region_id"] = json!(101);
            }
            "scatter-regions" => {
                conflict["region_ids"] = json!([102]);
            }
            _ => {
                conflict["region_id"] = json!(102);
            }
        }
        let before = request(url, "GET", "?object", Value::Null);
        let r = request(url, "POST", "", conflict.clone());
        if conflict["name"] == "scatter-regions" {
            assert_eq!(r, (200, json!({"processed-percentage":0,"error":""})));
        } else {
            assert_eq!(r.0, 500, "{conflict}: {r:?}");
        }
        assert_eq!(request(url, "GET", "?object", Value::Null), before);
        assert_eq!(request(url, "GET", "/102", Value::Null), sentinel_before);
        observations.push(json!(["command-conflict", conflict, r]));
        if input["name"] == "transfer-region"
            && let Some(store) = fault
        {
            let before = request(url, "GET", "?object", Value::Null);
            let key = Read::prefix(
                format!("/pd/{id}/alloc_id").as_bytes(),
                Consistency::Linearizable,
            );

            let current = client
                .alloc_id(pdpb::AllocIdRequest {
                    header: crate::grpc_region_lookup_scenarios_test::header(id),
                    count: 1,
                })
                .await
                .unwrap()
                .into_inner()
                .id;
            let snapshot = store.inner.read(key.clone()).await.unwrap();
            let end = u64::from_be_bytes(snapshot.entries[0].value.as_slice().try_into().unwrap());
            if end > current {
                let last = client
                    .alloc_id(pdpb::AllocIdRequest {
                        header: crate::grpc_region_lookup_scenarios_test::header(id),
                        count: (end - current).try_into().unwrap(),
                    })
                    .await
                    .unwrap()
                    .into_inner()
                    .id;
                assert_eq!(last, end);
            }
            let durable = store.inner.read(key.clone()).await.unwrap().entries;
            store.mode.store(5, std::sync::atomic::Ordering::SeqCst);
            let error = request(url, "POST", "", input.clone());
            assert_eq!(error.0, 500, "{error:?}");
            assert_eq!(store.mode.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(request(url, "GET", "?object", Value::Null), before);
            assert_eq!(store.inner.read(key).await.unwrap().entries, durable);
            println!(
                "real etcd allocation transaction failure, no admission or durable mutation: {error:?}"
            );
        }
        let result = request(url, "POST", "", input.clone());
        assert_eq!(result.0, 200, "{input}: {result:?}");
        observations.push(json!([input, result]));
        let objects = request(url, "GET", "?object", Value::Null);
        assert_eq!(objects.0, 200);
        let ops = objects.1.as_array().unwrap();
        assert!(ops.iter().any(|o| o["region_id"] == 102));
        assert_eq!(request(url, "GET", "/102", Value::Null), sentinel_before);
        if input["name"] != "scatter-region" && input["name"] != "scatter-regions" {
            let op = ops
                .iter()
                .find(|o| o["region_id"] == 100)
                .expect("created operator");
            assert_eq!(op["status"], 1);
            assert_eq!(op["region_epoch"], json!({"version":3,"conf_ver":1}));
            // Object contains no allocator IDs or clocks; compare every field.
            observations.push(json!(["object", op]));
            if input["name"] == "transfer-leader" {
                let mut stream =
                    tokio::time::timeout(std::time::Duration::from_secs(10), pending_stream)
                        .await
                        .unwrap()
                        .unwrap();
                let mut received = false;
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    while let Some(reply) = stream.message().await.unwrap() {
                        if reply.region_id == 100
                            && let Some(t) = reply.transfer_leader
                        {
                            assert_eq!(t.peer, Some(peers[1].clone()));
                            received = true;
                            break;
                        }
                    }
                })
                .await
                .unwrap();
                assert!(received);
                // Preserve the streaming task handle for subsequent iterations.
                pending_stream = tokio::spawn(async move { stream });
                let conflict = request(
                    url,
                    "POST",
                    "",
                    json!({"name":"transfer-leader","region_id":100,"to_store_id":3}),
                );
                assert_eq!(conflict.0, 500);
                observations.push(json!(["conflict", conflict]));
                assert_eq!(request(url, "GET", "?object", Value::Null), objects);
            }
        }
        if input["name"] == "merge-region" {
            assert!(ops.iter().any(|o| o["region_id"] == 101));
        }
        // Single cancellation removes a merge pair, but leaves unrelated regions.
        let remove = request(url, "DELETE", "/100", Value::Null);
        if ops.iter().any(|o| o["region_id"] == 100) {
            assert_eq!(remove, (200, json!("The pending operator is canceled.")));
            let status = request(url, "GET", "/100", Value::Null);
            assert!(
                status.1.as_str().unwrap().starts_with("status: CANCEL,"),
                "{status:?}"
            );
        }
        assert_eq!(request(url, "GET", "/102", Value::Null), sentinel_before);
        if input["name"] == "scatter-regions" {
            let _ = request(url, "DELETE", "/101", Value::Null);
        }
    }
    for input in [
        json!({"name":"transfer-leader","region_id":999,"to_store_id":2}),
        json!({"name":"transfer-peer","region_id":100,"from_store_id":9,"to_store_id":4}),
        json!({"name":"add-peer","region_id":100,"store_id":1}),
        json!({"name":"add-learner","region_id":100,"store_id":99}),
        json!({"name":"remove-peer","region_id":100,"store_id":9}),
        json!({"name":"split-region","region_id":100,"policy":"bad"}),
        json!({"name":"merge-region","source_region_id":100,"target_region_id":102}),
    ] {
        let before = request(url, "GET", "?object", Value::Null);
        let r = request(url, "POST", "", input.clone());
        assert_eq!(r.0, 500, "{input}: {r:?}");
        assert_eq!(request(url, "GET", "?object", Value::Null), before);
        observations.push(json!([input, r]));
    }
    for _ in 0..2 {
        let r = request(url, "DELETE", "", Value::Null);
        assert_eq!(r, (200, json!("All pending operators are canceled.")));
        observations.push(json!(["delete-all", r]));
    }
    assert_eq!(
        request(url, "GET", "?object", Value::Null),
        (200, json!([]))
    );
    let missing = request(url, "DELETE", "/999", Value::Null);
    assert_eq!(missing.0, 500);
    observations.push(json!(["delete-missing", missing]));
    for number in [
        json!(-1),
        json!(-0.5),
        json!(100.9),
        json!(1e100),
        json!(18446744073709551616.0),
        json!(9007199254740993u64),
    ] {
        let input = json!({"name":"transfer-leader","region_id":number,"to_store_id":2.9});
        let r = request(url, "POST", "", input.clone());
        assert_eq!(
            r.0,
            if number == json!(100.9) { 200 } else { 500 },
            "numeric Go conversion {input}: {r:?}"
        );
        observations.push(json!([input, r]));
        if number == json!(100.9) {
            assert_eq!(request(url, "DELETE", "/100", Value::Null).0, 200);
        }
    }
    for input in [
        json!({"name":"scatter-regions","region_ids":[],"retry_limit":1e100}),
        json!({"name":"scatter-regions","region_ids":[100.9],"retry_limit":0}),
        json!({"name":"split-region","region_id":100,"policy":"USEKEY","keys":["zz"]}),
        json!({"name":"transfer-region","region_id":100,"to_store_ids":[2,3,4]}),
        json!({"name":"transfer-region","region_id":100,"to_store_ids":["2"]}),
    ] {
        let before = request(url, "GET", "?object", Value::Null);
        let r = request(url, "POST", "", input.clone());
        if input["name"] == "scatter-regions" {
            assert_eq!(r.0, 200);
            let _ = request(url, "DELETE", "", Value::Null);
        } else {
            assert!(r.0 == 400 || r.0 == 500, "{input}: {r:?}");
            assert_eq!(request(url, "GET", "?object", Value::Null), before);
        }
        observations.push(json!([input, r]));
    }
    if let Some(store) = fault {
        assert_eq!(
            raft_before.unwrap(),
            store
                .inner
                .read(Read::prefix(
                    format!("/pd/{id}/raft/").as_bytes(),
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
        );
    }
    // Running operators belong to the term, not durable raft metadata. The
    // differential restarts both implementations with this admission outstanding.
    assert_eq!(
        request(
            url,
            "POST",
            "",
            json!({"name":"transfer-leader","region_id":100,"to_store_id":2})
        )
        .0,
        200
    );
    let pending = request(url, "GET", "?object", Value::Null);
    assert_eq!(pending.1.as_array().unwrap().len(), 1);
    observations.push(json!(["pending-before-restart", pending]));
    drop(tx);
    pending_stream.abort();
    let _ = pending_stream.await;
    json!(observations)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_063_differential() {
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
    let go_observations = observe(&go.url, id, None).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let go_restart = request(&go.url, "GET", "?object", Value::Null);
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7631,
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
    let rust = observe(&url, 7631, None).await;
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
                    cluster_id: 7631,
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
    let rust_restart = request(&restarted_url, "GET", "?object", Value::Null);
    restarted.shutdown().await.unwrap();
    assert_eq!(go_restart, rust_restart);
    println!("restart Go/Rust: {go_restart:?}; Go: {go_observations}\nRust: {rust}");
    assert_eq!(go_observations, rust);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_063_cancel_and_follower() {
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
                cluster_id: 7632,
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
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, 7632).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7632,
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
        request(&follower_url, "DELETE", "/999", Value::Null),
        request(&url, "DELETE", "/999", Value::Null)
    );
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            "pd-allow-follower-handle: true",
            "-X",
            "DELETE",
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
    let read = Read::prefix(b"/pd/7632/raft/", Consistency::Linearizable);
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
            b"DELETE /pd/api/v1/operators HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
