// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_compat_tests::{harness::Server, readonly_oracle};
use astersql_pd_id::Allocator;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_member::Member;
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation, StorageEndpoint};
use astersql_pd_storage_kv::EtcdMetaStore;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Command, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) fn dr_config() -> ReplicationModeConfig {
    let mut config = ReplicationModeConfig::default();
    config.replication_mode = "dr-auto-sync".into();
    config.dr_auto_sync.label_key = "zone".into();
    config.dr_auto_sync.primary = "one".into();
    config.dr_auto_sync.dr = "two".into();
    config.dr_auto_sync.primary_replicas = 1;
    config.dr_auto_sync.dr_replicas = 1;
    config
}

pub(crate) fn post(server: &Server, value: &Value) -> (u16, Value) {
    let output = Command::new("curl")
        .args(["-sS", "--max-time", "10", "-X", "POST", "--data-binary"])
        .arg(value.to_string())
        .args(["-w", "\n%{http_code}"])
        .arg(format!("{}/pd/api/v1/config/replication-mode", server.url))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    (code.parse().unwrap(), serde_json::from_str(body).unwrap())
}

pub(crate) fn pinned_go_binary() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    readonly_oracle::validate(
        "go",
        &binary,
        None,
        &root.join("tests/rust-compat/fixtures/health-member.json"),
    )
    .unwrap();
    let info = Command::new("go")
        .args(["version", "-m"])
        .arg(&binary)
        .output()
        .unwrap();
    assert!(info.status.success());
    readonly_oracle::validate_go_build_info(&String::from_utf8(info.stdout).unwrap()).unwrap();
    let admission = Command::new("gzip")
        .arg("-dc")
        .arg(root.join("docs/rust/control-plane-mcs/task-001-evidence.json.gz"))
        .output()
        .unwrap();
    assert!(admission.status.success());
    let admission: Value = serde_json::from_slice(&admission.stdout).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&binary).unwrap())),
        admission["binaries"]["go_pd"]["sha256"].as_str().unwrap()
    );
    binary
}

async fn go_loaded_state(seed: &[u8], expected_id: u64) -> Value {
    let binary = pinned_go_binary();
    let mut server = Server::start(&binary).unwrap();
    server.ready().unwrap();
    let normalized = post(&server, &json!({"replication-mode":"MAJORİTY"}));
    assert_eq!(normalized.0, 200);
    assert_eq!(
        server.get("/pd/api/v1/config/replication-mode").unwrap().1["replication-mode"],
        "MAJORİTY"
    );
    println!("Go normalization accepts and preserves MAJORİTY: {normalized:?}");
    assert_eq!(normalize_replication_mode("MAJORİTY"), "majority");
    assert_eq!(
        post(&server, &json!({"replication-mode":"majority"})).0,
        200
    );
    let before = server.get("/pd/api/v1/config/replication-mode").unwrap();
    let invalid = post(&server, &json!({"replication-mode":"invalid"}));
    assert_eq!(invalid.0, 500);
    assert_eq!(invalid.1, "invalid replication mode: invalid");
    assert_eq!(
        before,
        server.get("/pd/api/v1/config/replication-mode").unwrap()
    );
    assert_eq!(
        post(&server, &serde_json::to_value(dr_config()).unwrap()).0,
        200
    );
    let mut client = pdpb::pd_client::PdClient::connect(server.url.clone())
        .await
        .unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let backend = EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(3))
        .await
        .unwrap();
    backend
        .transact(
            vec![],
            vec![Mutation::put(
                ClusterPaths::new(id)
                    .replication_mode_path("dr-auto-sync")
                    .as_bytes(),
                seed,
            )],
        )
        .await
        .unwrap();
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
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
    assert!(response.header.unwrap().error.is_none());
    let rpc = response.replication_status.unwrap().dr_auto_sync.unwrap();
    assert_eq!(rpc.state_id, expected_id);
    let http = server.get("/pd/api/v1/replication_mode/status").unwrap();
    assert_eq!(http.0, 200);
    if expected_id == 0 {
        assert!(http.1["dr-auto-sync"].get("state_id").is_none());
    } else {
        assert_eq!(http.1["dr-auto-sync"]["state_id"], expected_id);
    }
    let observed = json!({"http":http,"invalid_config":invalid,"rpc_state_id":rpc.state_id,"rpc_available_stores":rpc.available_stores,"seed":String::from_utf8_lossy(seed)});
    std::fs::write(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join("mcs-004-go-observation.json"),
        serde_json::to_vec_pretty(&observed).unwrap(),
    )
    .unwrap();
    println!("Go mode observation: {observed}");
    server.stop().unwrap();
    observed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_004_behavior() {
    let seed = br#"{"state":"sync","state_id":42,"available_stores":[1]}"#;
    let go = go_loaded_state(seed, 42).await;
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let cancel = CancellationToken::new();
    let paths = ClusterPaths::new(404);
    let member = Member::bootstrap(store.clone(), cancel.clone(), 404, 1, b"mode".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap());
    store
        .transact(
            vec![],
            vec![Mutation::put(
                paths.replication_mode_path("dr-auto-sync").as_bytes(),
                seed,
            )],
        )
        .await
        .unwrap();
    let manager = ModeManager::new(
        dr_config(),
        Arc::new(StorageEndpoint::new(store)),
        paths,
        allocator,
        cancel,
    )
    .await
    .unwrap();
    let state = manager.snapshot().await.dr_auto_sync;
    assert_eq!(
        state.state_id,
        go["rpc_state_id"].as_u64().unwrap(),
        "manager must restore the actual Go state ID"
    );
    assert_eq!(
        state.state,
        go["http"][1]["dr-auto-sync"]["state"].as_str().unwrap()
    );
    assert_eq!(json!(state.available_stores), go["rpc_available_stores"]);
    leader.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_004_null_and_casefold_match_live_go() {
    for (seed, id) in [
        (r#"{"ſTATE":"sync","ſtate_ID":42}"#.as_bytes(), 42),
        (b"null".as_slice(), 0),
        (
            br#"{"STATE":"sync","state_ID":1,"STATE_ID":42,"available_stores":null}"#.as_slice(),
            42,
        ),
    ] {
        let go = go_loaded_state(seed, id).await;
        let parsed: Option<DRAutoSyncStatus> = serde_json::from_slice(seed).unwrap();
        let parsed = parsed.unwrap_or_default();
        assert_eq!(parsed.state_id, go["rpc_state_id"].as_u64().unwrap());
        assert_eq!(
            parsed.state,
            go["http"][1]["dr-auto-sync"]["state"].as_str().unwrap()
        );
    }
    for invalid in [
        "[]",
        "true",
        r#"{"state_id":-1}"#,
        r#"{"recover_progress":1e50}"#,
    ] {
        assert!(
            serde_json::from_str::<DRAutoSyncStatus>(invalid).is_err(),
            "{invalid}"
        );
    }
    let invalid = DRAutoSyncStatus {
        recover_progress: f32::NAN,
        ..Default::default()
    };
    assert!(serde_json::to_vec(&invalid).is_err());
}

use crate::fixture;

async fn local_state_contract(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let paths = ClusterPaths::new(405);
    let endpoint = Arc::new(StorageEndpoint::new(store.clone()));
    let member = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        405,
        1,
        b"local-mode".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap());
    let make = |config| {
        ModeManager::new(
            config,
            endpoint.clone(),
            paths.clone(),
            allocator.clone(),
            cancel.clone(),
        )
    };
    // Existing JSON null is present, so it must not initialize a fresh ID.
    let key = paths.replication_mode_path("dr-auto-sync");
    store
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"null")])
        .await
        .unwrap();
    let null = make(dr_config()).await.unwrap();
    assert_eq!(
        null.snapshot().await.dr_auto_sync,
        DRAutoSyncStatus::default()
    );
    // Empty/missing bytes initialize sync and persist the actual allocated ID.
    store
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"")])
        .await
        .unwrap();
    let initialized = make(dr_config()).await.unwrap();
    let initial = initialized.snapshot().await;
    assert_eq!(initial.dr_auto_sync.state, "sync");
    assert!(initial.dr_auto_sync.state_id > 0);
    assert_eq!(
        endpoint
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(initial.dr_auto_sync.clone())
    );
    assert_eq!(make(dr_config()).await.unwrap().snapshot().await, initial);
    // Even a corrupt DR record is irrelevant while the current mode is majority.
    store
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"broken")])
        .await
        .unwrap();
    assert!(make(dr_config()).await.is_err());
    let majority = ReplicationModeConfig {
        replication_mode: "majority".into(),
        ..Default::default()
    };
    let manager = make(majority.clone()).await.unwrap();
    assert_eq!(
        manager.snapshot().await.dr_auto_sync,
        DRAutoSyncStatus::default()
    );
    let invalid = ReplicationModeConfig {
        replication_mode: "invalid".into(),
        ..Default::default()
    };
    assert!(matches!(
        manager.update_config(invalid).await,
        Err(ModeError::InvalidMode(_))
    ));
    assert_eq!(manager.snapshot().await.config, majority);
    manager.update_config(dr_config()).await.unwrap();
    let recovered = manager.snapshot().await;
    assert_eq!(recovered.dr_auto_sync.state, "sync_recover");
    assert!(recovered.dr_auto_sync.state_id > initial.dr_auto_sync.state_id);
    assert!(recovered.dr_auto_sync.recover_start.is_some());
    let mut changed = dr_config();
    changed.dr_auto_sync.label_key = "other".into();
    manager.update_config(changed.clone()).await.unwrap();
    assert_eq!(
        manager.snapshot().await.dr_auto_sync,
        recovered.dr_auto_sync
    );
    cancel.cancel();
    assert!(matches!(
        manager.update_config(majority).await,
        Err(ModeError::Cancelled)
    ));
    assert_eq!(manager.snapshot().await.config, changed);
    leader.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_004_real_storage_and_cancel() {
    local_state_contract(Arc::new(MemoryStore::default())).await;
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    local_state_contract(store.clone()).await;
    let paths = ClusterPaths::new(405);
    let endpoint = StorageEndpoint::new(store.clone());
    let before = endpoint
        .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "started");
    assert_eq!(
        StorageEndpoint::new(Arc::new(fixture.store().await))
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        before
    );
    fixture.command("stop", "stopped");
    assert!(
        endpoint
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_004_write_failure_keeps_old_published_state() {
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let fixture = Arc::new(Mutex::new(fixture::Fixture::start()));
    let address = fixture.lock().unwrap().endpoint.clone();
    let store: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(&[address], Duration::from_millis(500))
            .await
            .unwrap(),
    );
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 406, 1, b"failure".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap());
    let paths = ClusterPaths::new(406);
    let endpoint = Arc::new(StorageEndpoint::new(store.clone()));
    let majority = ReplicationModeConfig {
        replication_mode: "majority".into(),
        ..Default::default()
    };
    let outage = Arc::new(AtomicBool::new(true));
    let clock: ModeClock = {
        let fixture = fixture.clone();
        let outage = outage.clone();
        Arc::new(move || {
            // The allocator has succeeded; stop the real backend before the state write.
            if outage.swap(false, Ordering::SeqCst) {
                fixture.lock().unwrap().command("stop", "stopped");
            }
            chrono::DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z").unwrap()
        })
    };
    let manager = ModeManager::new_with_clock(
        majority,
        endpoint.clone(),
        paths.clone(),
        allocator,
        cancel,
        clock,
    )
    .await
    .unwrap();
    let before = manager.snapshot().await;
    let error = manager.update_config(dr_config()).await.unwrap_err();
    assert!(matches!(error, ModeError::Storage(_)), "{error:?}");
    assert_eq!(manager.snapshot().await, before);
    fixture.lock().unwrap().command("start", "started");
    assert!(
        endpoint
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap()
            .is_none()
    );
    manager.update_config(dr_config()).await.unwrap();
    let recovered = manager.snapshot().await;
    assert_eq!(
        recovered.dr_auto_sync.state_id, 2,
        "failed publication still consumes its allocated ID"
    );
    assert_eq!(recovered.dr_auto_sync.state, "sync_recover");
    assert_eq!(
        endpoint
            .load_replication_status::<DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(recovered.dr_auto_sync.clone())
    );
    leader.resign().await.unwrap();

    let proxy = crate::mode_proxy_test::Proxy::start().await;
    let binary = pinned_go_binary();
    let advertised = format!("--advertise-client-urls=http://{}", proxy.address);
    let mut go = Server::start_with_args(&binary, &[&advertised]).unwrap();
    proxy.target(go.url.trim_start_matches("http://").parse().unwrap());
    go.ready().unwrap();
    let mut client = pdpb::pd_client::PdClient::connect(go.url.clone())
        .await
        .unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    assert!(
        client
            .bootstrap(bootstrap_request(id))
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    let before = go.get("/pd/api/v1/config/replication-mode").unwrap();
    let key = ClusterPaths::new(id).replication_mode_path("dr-auto-sync");
    proxy.block(key.as_bytes());
    let failed = post(&go, &serde_json::to_value(dr_config()).unwrap());
    assert_eq!(
        failed.0, 500,
        "failed state persistence must reach the public error: {failed:?}"
    );
    assert_eq!(
        go.get("/pd/api/v1/config/replication-mode").unwrap(),
        before
    );
    assert_eq!(
        go.get("/pd/api/v1/replication_mode/status").unwrap().1["mode"],
        "majority"
    );
    let backend = EtcdMetaStore::connect(std::slice::from_ref(&go.url), Duration::from_secs(2))
        .await
        .unwrap();
    let rows = backend
        .read(astersql_pd_storage_domain::Read::range(
            key.as_bytes(),
            b"",
            astersql_pd_storage_domain::Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert!(rows.entries.is_empty());
    let attempted: Vec<Value> = proxy
        .rejected()
        .iter()
        .map(|bytes| {
            assert_eq!(bytes[0], 0, "uncompressed real gRPC request");
            let begin = bytes.iter().position(|&b| b == b'{').unwrap();
            let end = bytes.iter().rposition(|&b| b == b'}').unwrap();
            serde_json::from_slice(&bytes[begin..=end]).unwrap()
        })
        .collect();
    assert!(
        !attempted.is_empty(),
        "the fault must intercept an actual state write"
    );
    assert!(
        attempted
            .iter()
            .all(|state| state["state"] == "sync_recover"
                && state["state_id"].as_u64().is_some_and(|id| id > 0))
    );
    let mut observation = json!({"failure":failed,"attempted_state_writes":attempted,"public_config_restored":true,"persisted_state_absent":true});
    proxy.unblock();
    let started = chrono::Utc::now();
    assert_eq!(
        post(&go, &serde_json::to_value(dr_config()).unwrap()).0,
        200
    );
    let completed = chrono::Utc::now();
    let rows = backend
        .read(astersql_pd_storage_domain::Read::range(
            key.as_bytes(),
            b"",
            astersql_pd_storage_domain::Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(rows.entries.len(), 1);
    let raw: Value = serde_json::from_slice(&rows.entries[0].value).unwrap();
    let mut go_state: DRAutoSyncStatus = serde_json::from_value(raw.clone()).unwrap();
    let time = go_state.recover_start.unwrap();
    assert!(
        time >= started - chrono::Duration::seconds(1)
            && time <= completed + chrono::Duration::seconds(1)
    );
    // This is the only normalized field: Rust uses the explicitly controlled clock.
    go_state.recover_start = recovered.dr_auto_sync.recover_start;
    assert_eq!(go_state, recovered.dr_auto_sync);
    let config = go.get("/pd/api/v1/config/replication-mode").unwrap();
    assert_eq!(
        config,
        (200, serde_json::to_value(&recovered.config).unwrap())
    );
    let status = go.get("/pd/api/v1/replication_mode/status").unwrap();
    assert_eq!(status.0, 200);
    assert_eq!(status.1["dr-auto-sync"]["state_id"], 2);
    assert_eq!(status.1["dr-auto-sync"]["state"], "sync_recover");
    observation["recovery"] =
        json!({"persisted":raw,"http":status,"config":config,"rust_state":recovered.dr_auto_sync});
    println!("Go real persistence failure and recovery: {observation}");
    std::fs::write(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join("mcs-004-go-write-failure.json"),
        serde_json::to_vec_pretty(&observation).unwrap(),
    )
    .unwrap();
    go.stop().unwrap();
    proxy.stop().await;
}

pub(crate) fn bootstrap_request(id: u64) -> pdpb::BootstrapRequest {
    pdpb::BootstrapRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        }),
        store: Some(metapb::Store {
            id: 1,
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
    }
}
