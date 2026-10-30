// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_067_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 767,
                member_id: 1,
                name: "region-schedule".into(),
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
        crate::http_member_test::request(&url, "POST", "/pd/api/v1/regions/split", "{}"),
        (
            500,
            serde_json::json!(
                "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
            )
        )
    );
    node.shutdown().await.unwrap();
}

use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
#[path = "http_region_schedule_client_test.rs"]
mod http_region_schedule_client;
pub(crate) use http_region_schedule_client::request;
async fn observe(url: &str, id: u64) -> Value {
    let mut out = vec![];
    // Freeze cold-service validation and bootstrap error precedence.
    for (path, body) in [
        ("accelerate-schedule", r#"{"start_key":"","end_key":""}"#),
        ("accelerate-schedule/batch", "[]"),
        ("accelerate-schedule/batch", "[1]"),
        ("accelerate-schedule/batch", "[[]]"),
        ("accelerate-schedule/batch", "[null]"),
        ("scatter", r#"{"regions_id":[999]}"#),
        ("split", r#"{"split_keys":["61"]}"#),
    ] {
        out.push(json!(["cold", path, request(url, path, body).await]));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    // A real region at 2^63 makes the oracle's out-of-range float conversion observable.
    let big_id = 1u64 << 63;
    let peer = astersql_pd_proto::metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(pdpb::RegionHeartbeatRequest {
        header: super::grpc_region_lookup_scenarios_test::header(id),
        region: Some(astersql_pd_proto::metapb::Region {
            id: big_id,
            start_key: b"zzzz".to_vec(),
            region_epoch: Some(astersql_pd_proto::metapb::RegionEpoch {
                version: 3,
                conf_ver: 1,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        }),
        leader: Some(peer),
        approximate_size: 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let r = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: super::grpc_region_lookup_scenarios_test::header(id),
                    region_id: big_id,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if r.region.is_some() && r.leader.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    drop(stream);
    // Disable autonomous balance/checker work; explicit scatter/split still admit.
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
    for (path, body) in [
        ("accelerate-schedule", "{}"),
        ("accelerate-schedule?limit=nope", "{}"),
        (
            "accelerate-schedule?limit=0",
            r#"{"start_key":"61","end_key":"7a"}"#,
        ),
        (
            "accelerate-schedule?limit=100000",
            r#"{"start_key":"61","end_key":"7a"}"#,
        ),
        ("accelerate-schedule", r#"{"start_key":1,"end_key":""}"#),
        ("accelerate-schedule", r#"{"start_key":"z","end_key":""}"#),
        ("accelerate-schedule", r#"{"start_key":"0","end_key":""}"#),
        (
            "accelerate-schedule",
            r#"{"start_key":"61","end_key":"7a"}"#,
        ),
        (
            "accelerate-schedule?limit=oops",
            r#"{"start_key":"","end_key":""}"#,
        ),
        (
            "accelerate-schedule?limit=9223372036854775808",
            r#"{"start_key":"","end_key":""}"#,
        ),
        (
            "accelerate-schedule?limit=-1",
            r#"{"start_key":"61","end_key":"7a"}"#,
        ),
        ("accelerate-schedule/batch", "[]"),
        ("accelerate-schedule/batch", "[1]"),
        ("accelerate-schedule/batch", "[[]]"),
        ("accelerate-schedule/batch", "[null]"),
        ("accelerate-schedule/batch", "null"),
        ("accelerate-schedule/batch", "[{}]"),
        ("accelerate-schedule/batch", r#"[{"start_key":1}]"#),
        (
            "accelerate-schedule/batch",
            r#"[{"start_key":"z","end_key":""}]"#,
        ),
        (
            "accelerate-schedule/batch?limit=1",
            r#"[{"start_key":"61","end_key":"6d"},{"start_key":"6d","end_key":"7a"}]"#,
        ),
        ("scatter", "{}"),
        ("scatter", r#"{"regions_id":[1e100],"retry_limit":0}"#),
        (
            "scatter",
            r#"{"regions_id":[18446744073709551616],"retry_limit":0}"#,
        ),
        ("scatter", r#"{"regions_id":[]}"#),
        ("scatter", r#"{"regions_id":["10"]}"#),
        ("scatter", r#"{"regions_id":[999],"retry_limit":0}"#),
        ("scatter", r#"{"start_key":"z","end_key":""}"#),
        (
            "scatter",
            r#"{"start_key":"ff","end_key":"ffff","retry_limit":0}"#,
        ),
        ("scatter", r#"{"regions_id":[10,999],"retry_limit":0}"#),
        ("split", "{}"),
        ("split", r#"{"split_keys":["62"],"retry_limit":1e100}"#),
        ("split", r#"{"split_keys":null}"#),
        ("split", r#"{"split_keys":1}"#),
        ("split", r#"{"split_keys":[]}"#),
        ("split", r#"{"split_keys":[1]}"#),
        ("split", r#"{"split_keys":["z"]}"#),
        ("split", r#"{"split_keys":["0"]}"#),
        ("split", r#"{"split_keys":["61"],"retry_limit":-1}"#),
        ("split", r#"{"split_keys":["61"],"retry_limit":0}"#),
        ("split", "{"),
        ("split", "[]"),
        ("split", "true"),
        ("split", "123"),
        ("accelerate-schedule/batch", "{}"),
    ] {
        let response = request(url, path, body).await;
        if path == "scatter" && (body.contains("1e100") || body.contains("18446744073709551616")) {
            assert_eq!(
                response,
                (200, json!({"processed-percentage":0})),
                "out-of-range number must select the existing 2^63 region"
            );
        }
        out.push(json!([path, body, response]));
    }
    let scatter = super::grpc_scatter_scenarios_test::observe_http(&mut client, id, url).await;
    out.push(json!(["scatter-operators", scatter]));
    out.push(json!([
        "scatter-range",
        request(
            url,
            "scatter",
            r#"{"start_key":"64","end_key":"67","group":"task029","retry_limit":0}"#
        )
        .await
    ]));
    assert_eq!(
        crate::http_member_test::request(url, "DELETE", "/pd/api/v1/operators", "").0,
        200
    );
    let split = super::grpc_split_regions_scenarios_test::observe_http(&mut client, id, url).await;
    out.push(json!(["split-operators", split]));
    json!(out)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_067_operations() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7672,
                member_id: 1,
                name: "region-schedule".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let results = observe(&url, 7672).await;
    println!("Rust: {results}");
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store);
    let paths = astersql_pd_keypath::ClusterPaths::new(7672);
    for id in [101, 102, 103] {
        assert_eq!(
            endpoint.load_region(&paths, id).await.unwrap().unwrap().id,
            id
        );
    }
    node.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_067_differential() {
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
    go.stop().unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7673,
                member_id: 1,
                name: "region-schedule".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let rust = observe(&url, 7673).await;
    node.shutdown().await.unwrap();
    println!("Go: {oracle}\nRust: {rust}");
    assert_eq!(oracle, rust);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_067_follower_pending_and_cancel() {
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
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7674,
                member_id: 1,
                name: "leader".into(),
                client_url: url.clone(),
                peer_url: "http://peer1".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 7674).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7674,
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
    let replicated = |start: &str, end: &str| {
        crate::http_member_test::request(
            &url,
            "GET",
            &format!("/pd/api/v1/regions/replicated?startKey={start}&endKey={end}"),
            "",
        )
    };
    assert_eq!(replicated("61", "6d"), (200, json!("INPROGRESS")));
    assert!(
        node.cluster()
            .pending_processed_regions()
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        request(
            &follower_url,
            "accelerate-schedule?limit=1",
            r#"{"start_key":"61","end_key":"7a"}"#
        )
        .await
        .0,
        200
    );
    assert_eq!(
        node.cluster().pending_processed_regions().await.unwrap(),
        vec![10]
    );
    assert_eq!(replicated("6d", "7a"), (200, json!("INPROGRESS")));
    assert_eq!(
        request(
            &follower_url,
            "accelerate-schedule/batch?limit=1",
            r#"[{"start_key":"6d","end_key":"7a"},{"start_key":"6d","end_key":"7a"}]"#
        )
        .await
        .0,
        200
    );
    assert_eq!(
        node.cluster().pending_processed_regions().await.unwrap(),
        vec![10, 11]
    );
    assert_eq!(
        request(
            &follower_url,
            "scatter",
            r#"{"regions_id":[10,999],"retry_limit":0}"#
        )
        .await,
        (200, json!({"processed-percentage":0}))
    );
    follower.shutdown().await.unwrap();
    let read = Read::range(b"/pd/7674/config", b"", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap().entries;
    // Real transaction rejection must not change config or corrupt the pending queue.
    store.mode.store(3, Ordering::SeqCst);
    assert_eq!(
        crate::http_member_test::request(
            &url,
            "POST",
            "/pd/api/v1/config/schedule",
            r#"{"halt-scheduling":"true"}"#
        )
        .0,
        500
    );
    assert_eq!(before, inner.read(read.clone()).await.unwrap().entries);
    assert_eq!(
        node.cluster().pending_processed_regions().await.unwrap(),
        vec![10, 11]
    );
    // Queue acceleration behind a blocked real etcd transaction, then cancel the host.
    store.mode.store(4, Ordering::SeqCst);
    let post_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "--data-binary",
                r#"{"halt-scheduling":"true"}"#,
                "-w",
                "\n%{http_code}",
                &format!("{post_url}/pd/api/v1/config/schedule"),
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let body = r#"{"start_key":"7a7a","end_key":""}"#;
    socket.write_all(format!("POST /pd/api/v1/regions/accelerate-schedule HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    let mut response = vec![];
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
    assert!(
        response.starts_with("HTTP/1.1 500") && response.contains("server not started"),
        "{response}"
    );
    assert!(pending.await.unwrap().ends_with("500"));
    node.shutdown().await.unwrap();
    assert_eq!(before, inner.read(read).await.unwrap().entries);
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                inner,
                NodeConfig {
                    cluster_id: 7674,
                    member_id: 1,
                    name: "leader".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer1".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    assert_eq!(replicated("61", "6d"), (200, json!("INPROGRESS")));
    assert!(
        restarted
            .cluster()
            .pending_processed_regions()
            .await
            .unwrap()
            .is_empty()
    );
    restarted.shutdown().await.unwrap();
    println!(
        "follower range limit and deduplicated batch changed canonical pending state; real transaction rejection/cancellation preserved persisted configuration: {response}"
    );
}
