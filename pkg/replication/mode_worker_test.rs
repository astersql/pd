// Copyright 2026 AsterSQL.
use crate::async_confirm_test::go_async_scenario;
use crate::async_wait_test::{rule_manager, rules, stores};
use crate::mode_state_test::bootstrap_request;
use astersql_pd_proto::{
    pdpb,
    replication_modepb::{DrAutoSyncState, StoreDrAutoSyncStatus},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn get(url: &str, path: &str) -> (u16, String) {
    let output = Command::new("curl")
        .args(["-sS", "--max-time", "5", "-w", "\n%{http_code}"])
        .arg(format!("{url}{path}"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    (code.parse().unwrap(), body.into())
}
fn metric(text: &str, name: &str) -> f64 {
    text.lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{name} "))
                .map(|s| s.parse().unwrap())
        })
        .unwrap()
}
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_008_behavior() {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let metas = stores(now);
    let (mut go, _, _, state) = go_async_scenario(&metas).await;
    let config = go.get("/pd/api/v1/config").unwrap().1;
    let go_file = PathBuf::from(config["data-dir"].as_str().unwrap()).join("DR_STATE");
    let expected_file = json!({"state":"async","state_id":3,"available_stores":[100,200,300,400]});
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if std::fs::read(&go_file)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                == Some(expected_file.clone())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let go_metrics = get(&go.url, "/metrics");
    assert_eq!(go_metrics.0, 200);
    assert_eq!(metric(&go_metrics.1, "pd_replication_dr_state"), 3.0);
    assert_eq!(metric(&go_metrics.1, "pd_replication_dr_state_id"), 3.0);
    assert!(metric(&go_metrics.1, "pd_replication_dr_tick_total") > 0.0);
    println!(
        "Go worker persisted state: {state}; replicated file: {expected_file}; metrics state=3,id=3"
    );
    let go_compact = go.get("/pd/api/v1/cluster/status").unwrap().1["replication_status"].clone();
    assert!(
        go_compact
            .as_str()
            .is_some_and(|s| s.contains("DR_AUTO_SYNC"))
    );
    println!("Go compact replication status: {go_compact}");
    go.stop().unwrap();

    let fixture = crate::fixture::Fixture::start();
    let backend: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = astersql_pd_keypath::ClusterPaths::new(808);
    let mut config = crate::mode_state_test::dr_config();
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    config.dr_auto_sync.pause_region_split = true;
    astersql_pd_storage_endpoint::StorageEndpoint::new(backend.clone()).save_config(&paths,&json!({"replication-mode":config,"replication":{"max-replicas":6},"schedule":{"halt-scheduling":"true","replica-schedule-limit":0,"region-schedule-limit":0,"leader-schedule-limit":0,"merge-schedule-limit":0}})).await.unwrap();
    rule_manager(backend.clone(), paths.clone(), rules()).await;
    let directory = Directory(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("mcs008-{}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let mut host_config = astersql_pd_server_config::Config::default();
    host_config.scalars.data_dir = directory.0.to_str().unwrap().into();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let registry = Arc::new(prometheus::Registry::new());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_listener_config(host_config)
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 808,
                member_id: 8,
                name: "mcs008".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    let mut boot = bootstrap_request(808);
    boot.store = Some(astersql_pd_server::core_store_adapter::store_to_proto(
        metas[0].clone(),
    ));
    boot.region.as_mut().unwrap().id = 5001;
    boot.region.as_mut().unwrap().peers[0].id = 5002;
    boot.region.as_mut().unwrap().peers[0].store_id = 100;
    let response = client.bootstrap(boot).await.unwrap().into_inner();
    assert!(
        response.header.unwrap().error.is_none(),
        "DR bootstrap must create the real manager"
    );
    for meta in &metas[1..] {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 808,
                    ..Default::default()
                }),
                store: Some(astersql_pd_server::core_store_adapter::store_to_proto(
                    meta.clone(),
                )),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
    }
    let initial = get(&url, "/pd/api/v1/replication_mode/status");
    assert_eq!(initial.0, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&initial.1).unwrap()["dr-auto-sync"]["state"],
        "sync"
    );
    tokio::time::timeout(Duration::from_secs(75), async {
        loop {
            let response = get(&url, "/pd/api/v1/replication_mode/status");
            assert_eq!(response.0, 200);
            let v: Value = serde_json::from_str(&response.1).unwrap();
            if v["dr-auto-sync"]["state"] == "async_wait" {
                assert_eq!(v["dr-auto-sync"]["state_id"], 2);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    for id in [100, 200, 300, 400] {
        let r = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 808,
                    ..Default::default()
                }),
                stats: Some(pdpb::StoreStats {
                    store_id: id,
                    capacity: 1_000_000_000,
                    available: 900_000_000,
                    used_size: 100_000_000,
                    ..Default::default()
                }),
                dr_autosync_status: Some(StoreDrAutoSyncStatus {
                    state: DrAutoSyncState::AsyncWait as i32,
                    state_id: 2,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(r.header.unwrap().error.is_none());
        assert!(
            r.replication_status
                .unwrap()
                .dr_auto_sync
                .unwrap()
                .pause_region_split
        );
    }
    let file = directory.0.join("DR_STATE");
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if std::fs::read(&file)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                == Some(expected_file.clone())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let metrics = get(&url, "/metrics");
    assert_eq!(metric(&metrics.1, "pd_replication_dr_state"), 3.0);
    assert_eq!(metric(&metrics.1, "pd_replication_dr_state_id"), 3.0);
    assert!(metric(&metrics.1, "pd_replication_dr_tick_total") > 0.0);
    let cluster_status = get(&url, "/pd/api/v1/cluster/status");
    assert_eq!(cluster_status.0, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&cluster_status.1).unwrap()["replication_status"],
        go_compact
    );
    let follower_dir = directory.0.join("follower");
    std::fs::create_dir_all(&follower_dir).unwrap();
    let mut follower_config = astersql_pd_server_config::Config::default();
    follower_config.scalars.data_dir = follower_dir.to_str().unwrap().into();
    let follower_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", follower_listener.local_addr().unwrap());
    let follower_registry = Arc::new(prometheus::Registry::new());
    let mut follower = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_listener_config(follower_config)
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 808,
                member_id: 9,
                name: "mcs008-follower".into(),
                client_url: follower_url.clone(),
                peer_url: "http://127.0.0.1:2381".into(),
            },
            follower_listener,
            follower_registry.clone(),
        )
        .await
        .unwrap();
    follower.ready().await.unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if std::fs::read(follower_dir.join("DR_STATE"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                == Some(expected_file.clone())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let row = backend
        .read(Read::prefix(
            paths.replication_mode_path("dr-auto-sync").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let mut persisted: Value = serde_json::from_slice(&row.entries[0].value).unwrap();
    assert!(persisted["async_start"].is_string());
    persisted["async_start"] = state["async_start"].clone();
    assert_eq!(persisted, state);
    let before = row.entries[0].clone();
    node.shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let after = backend
        .read(Read::prefix(
            paths.replication_mode_path("dr-auto-sync").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(after.entries[0].mod_revision, before.mod_revision);
    assert_eq!(after.entries[0].value, before.value);
    assert!(
        tokio::net::TcpStream::connect(url.trim_start_matches("http://"))
            .await
            .is_err()
    );
    let mut follower_client = pdpb::pd_client::PdClient::connect(follower_url.clone())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let members = follower_client
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner();
            if members.leader.is_some_and(|m| m.member_id == 9) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let restored = get(&follower_url, "/pd/api/v1/replication_mode/status");
    assert_eq!(restored.0, 200);
    let restored: Value = serde_json::from_str(&restored.1).unwrap();
    assert_eq!(restored["dr-auto-sync"]["state"], "async");
    assert_eq!(restored["dr-auto-sync"]["state_id"], 3);
    follower.shutdown().await.unwrap();
    assert!(follower_registry.gather().is_empty());
    assert!(
        !registry
            .gather()
            .iter()
            .any(|m| m.name() == "pd_replication_dr_state"),
        "listener-owned registration must drain after shutdown"
    );
}

fn post_bytes(url: &str, path: &str, bytes: &[u8]) -> (u16, Vec<u8>) {
    use std::io::Write;
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            "POST",
            "--data-binary",
            "@-",
            "-w",
            "\n%{http_code}",
        ])
        .arg(format!("{url}{path}"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let split = output.stdout.iter().rposition(|b| *b == b'\n').unwrap();
    (
        std::str::from_utf8(&output.stdout[split + 1..])
            .unwrap()
            .parse()
            .unwrap(),
        output.stdout[..split].to_vec(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_008_file_boundary_matches_go() {
    let binary = crate::mode_state_test::pinned_go_binary();
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let go_dir = PathBuf::from(
        go.get("/pd/api/v1/config").unwrap().1["data-dir"]
            .as_str()
            .unwrap(),
    );
    let fixture = crate::fixture::Fixture::start();
    let backend: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let directory = Directory(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("mcs008-file-{}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let mut config = astersql_pd_server_config::Config::default();
    config.scalars.data_dir = directory.0.to_str().unwrap().into();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_listener_config(config)
        .start(
            backend.clone(),
            NodeConfig {
                cluster_id: 881,
                member_id: 81,
                name: "file-boundary".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let oracle = go.get("/pd/api/v1/replication_mode/status").unwrap();
    let rust = get(&url, "/pd/api/v1/replication_mode/status");
    println!("pre-bootstrap replication status Go={oracle:?}, Rust={rust:?}");
    assert_eq!(rust.0, oracle.0);
    assert_eq!(serde_json::from_str::<Value>(&rust.1).unwrap(), oracle.1);
    let mut cases = vec![
        ("DR_STATE", b"{".to_vec(), 400),
        ("other", b"null".to_vec(), 500),
        ("DR_STATE", b"{\"x\":1}\n".to_vec(), 200),
        ("DR_STATE", b"null".to_vec(), 200),
        ("DR_STATE", b"\"\\ud800\"".to_vec(), 200),
        ("DR_STATE", vec![b'"', 0xff, b'"'], 200),
        ("DR_STATE", b"1e10000".to_vec(), 200),
    ];
    for (depth, expected) in [(10_000, 200), (10_001, 400)] {
        let mut body = vec![b'['; depth];
        body.extend_from_slice(b"0");
        body.extend(vec![b']'; depth]);
        cases.push(("DR_STATE", body, expected));
    }
    for (name, body, expected) in cases {
        let path = format!("/pd/api/v1/admin/persist-file/{name}");
        let oracle = post_bytes(&go.url, &path, &body);
        assert_eq!(oracle.0, expected, "Go body length {}", body.len());
        let rust = post_bytes(&url, &path, &body);
        assert_eq!(
            rust,
            oracle,
            "same JSON syntax/file error boundary for {} bytes",
            body.len()
        );
        if expected == 200 {
            assert_eq!(std::fs::read(directory.0.join("DR_STATE")).unwrap(), body);
            assert_eq!(std::fs::read(go_dir.join("DR_STATE")).unwrap(), body);
        }
        assert!(!directory.0.join("other").exists());
        println!(
            "file boundary {name}, bytes={}, status={expected}",
            body.len()
        );
    }
    let file = directory.0.join("DR_STATE");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    // Use the normal dependency type across the server boundary in this unit-test crate.
    let mut config = astersql_pd_server_config::ReplicationModeConfig::default();
    config.replication_mode = "dr-auto-sync".into();
    let mut snapshot = astersql_pd_server::replication_mode::ModeSnapshot {
        config,
        dr_auto_sync: Default::default(),
    };
    snapshot.dr_auto_sync.state = "async".into();
    snapshot.dr_auto_sync.state_id = 7;
    let mut cache = std::collections::BTreeMap::new();
    assert!(
        astersql_pd_server::replication_mode::replicate(
            backend.as_ref(),
            881,
            &snapshot,
            &Default::default(),
            &mut cache,
            CancellationToken::new()
        )
        .await
        .is_err()
    );
    assert!(cache.is_empty());
    assert!(file.is_dir());
    std::fs::remove_dir(&file).unwrap();
    astersql_pd_server::replication_mode::replicate(
        backend.as_ref(),
        881,
        &snapshot,
        &Default::default(),
        &mut cache,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let replicated: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(replicated, json!({"state":"async","state_id":7}));
    assert_eq!(cache.get(&81), Some(&7));
    node.shutdown().await.unwrap();
    assert!(registry.gather().is_empty());
    go.stop().unwrap();
}
