// Copyright 2026 AsterSQL.
use crate::ModeManager;
use crate::mode_state_test::{dr_config, pinned_go_binary, post};
use astersql_pd_compat_tests::harness::Server;
use astersql_pd_core::{NodeState, StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_id::Allocator;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_member::Member;
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{GroupBundle, Rule, RuleManager};
use astersql_pd_storage_domain::{Consistency, MemoryStore, MetaStore, Read, StorageEndpoint};
use astersql_pd_storage_kv::EtcdMetaStore;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) fn stores(now: i64) -> Vec<StoreMeta> {
    (0..7)
        .map(|i| StoreMeta {
            id: (i + 1) * 100,
            address: format!("127.0.0.1:{}", 21000 + i),
            version: "8.5.0".into(),
            node_state: NodeState::Serving,
            labels: vec![StoreLabel {
                key: "zone".into(),
                value: if i < 5 { "one" } else { "two" }.into(),
            }],
            last_heartbeat: if i < 4 || i == 5 {
                now + 86_400_000_000_000
            } else {
                0
            },
            ..Default::default()
        })
        .collect()
}

pub(crate) fn rules() -> Vec<Rule> {
    [ ("primary","one",4), ("dr","two",1) ].into_iter().map(|(id,zone,count)| {
        Rule::from_json(json!({"group_id":"dr","id":id,"role":"voter","count":count,"label_constraints":[{"key":"zone","op":"in","values":[zone]}]}).to_string().as_bytes()).unwrap()
    }).collect()
}

pub(crate) async fn rule_manager(
    store: Arc<dyn MetaStore>,
    paths: ClusterPaths,
    rules: Vec<Rule>,
) -> RuleManager {
    let mut manager = RuleManager::new(Arc::new(StorageEndpoint::new(store)), paths);
    manager.set_key_type("raw");
    let mut config = ProviderSnapshot::<()>::default();
    config.replication.max_replicas = 6;
    manager
        .initialize(6, &[], "", false, &config, &[])
        .await
        .unwrap();
    manager
        .set_all_group_bundles(
            vec![GroupBundle {
                id: "dr".into(),
                rules: Some(rules.into_iter().map(Some).collect()),
                ..Default::default()
            }],
            true,
            1,
            &config,
            &[],
        )
        .await
        .unwrap();
    manager
}

struct File(PathBuf);
impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(crate) async fn go_wait_scenario(
    stores: &[StoreMeta],
    rules: Vec<Rule>,
    expected: &str,
) -> (Server, Arc<dyn MetaStore>, ClusterPaths, Value) {
    let binary = pinned_go_binary();
    let file = File(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("mcs005-{}.toml", std::process::id())),
    );
    std::fs::write(&file.0, "[pd-server]\nkey-type = 'raw'\n[schedule]\nreplica-schedule-limit = 0\nregion-schedule-limit = 0\nleader-schedule-limit = 0\nmerge-schedule-limit = 0\n").unwrap();
    let arg = format!("--config={}", file.0.display());
    let mut server = Server::start_with_args(&binary, &[&arg]).unwrap();
    server.ready().unwrap();
    let mut config = dr_config();
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    assert_eq!(
        post(&server, &serde_json::to_value(&config).unwrap()).0,
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
    let backend: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&server.url), Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let paths = ClusterPaths::new(id);
    let wire = astersql_pd_storage_endpoint::StorageEndpoint::new(backend.clone());
    for store in stores {
        wire.save_store_meta(&paths, store).await.unwrap();
    }
    let rule_manager = rule_manager(backend.clone(), paths.clone(), rules).await;
    let key = paths.replication_mode_path("dr-auto-sync");
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(astersql_pd_server::core_store_adapter::store_to_proto(
                stores[0].clone(),
            )),
            region: Some(metapb::Region {
                id: 5001,
                peers: vec![metapb::Peer {
                    id: 5002,
                    store_id: stores[0].id,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    println!(
        "Go startup state: {:?}; rules: {:?}",
        server.get("/pd/api/v1/replication_mode/status").unwrap(),
        server.get("/pd/api/v1/config/rules").unwrap()
    );
    let target = expected.to_owned();
    let observed = tokio::time::timeout(Duration::from_secs(75), async {
        loop {
            let response = server.get("/pd/api/v1/replication_mode/status").unwrap();
            if response.0 == 200 && response.1["dr-auto-sync"]["state"] == target {
                break response.1;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let rows = backend
        .read(Read::prefix(key.as_bytes(), Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(rows.entries.len(), 1);
    let persisted: Value = serde_json::from_slice(&rows.entries[0].value).unwrap();
    let mut previous = Read::prefix(key.as_bytes(), Consistency::Linearizable);
    previous.revision = Some(rows.entries[0].mod_revision - 1);
    let previous = backend.read(previous).await.unwrap();
    assert_eq!(previous.entries.len(), 1);
    let prior: Value = serde_json::from_slice(&previous.entries[0].value).unwrap();
    assert_eq!(prior, json!({"state":"sync","state_id":1}));
    let events = vec![prior, persisted.clone()];
    let second = server.get("/pd/api/v1/replication_mode/status").unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        server.get("/pd/api/v1/replication_mode/status").unwrap(),
        second,
        "unchanged live stores without DR acknowledgments must not advance"
    );
    let data = json!({"http":observed,"persisted":persisted,"history":events,"rules":rule_manager.get_all_rules(),"stores":stores.iter().map(|s|json!({"id":s.id,"last_heartbeat":s.last_heartbeat})).collect::<Vec<_>>()});
    println!("Go sync-to-wait: {data}");
    std::fs::write(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("mcs-005-go.json"),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
    (server, backend, paths, data)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_005_behavior() {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let metas = stores(now);
    let rules = rules();
    let (mut go_server, _, _, go) = go_wait_scenario(&metas, rules.clone(), "async_wait").await;
    go_server.stop().unwrap();
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let paths = ClusterPaths::new(505);
    let manager_rules = rule_manager(store.clone(), paths.clone(), rules).await;
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), 505, 1, b"sync".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap());
    let mut config = dr_config();
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    let manager = ModeManager::new(
        config,
        Arc::new(StorageEndpoint::new(store)),
        paths,
        allocator,
        cancel,
    )
    .await
    .unwrap();
    let infos: Vec<_> = metas
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    manager
        .tick_sync(&infos, &manager_rules.get_all_rules(), now as i128)
        .await
        .unwrap();
    let actual = serde_json::to_value(manager.snapshot().await.dr_auto_sync).unwrap();
    assert_eq!(
        actual, go["persisted"],
        "the real quorum must publish the same state as Go"
    );
    leader.resign().await.unwrap();
}

pub(crate) async fn local_manager(
    store: Arc<dyn MetaStore>,
    id: u64,
) -> (
    ModeManager,
    Arc<astersql_pd_member::Leadership>,
    CancellationToken,
    ClusterPaths,
) {
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(store.clone(), cancel.clone(), id, 1, b"edge".to_vec())
        .await
        .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let allocator =
        Arc::new(Allocator::new(store.clone(), leader.clone(), cancel.clone(), 10).unwrap());
    let paths = ClusterPaths::new(id);
    let mut config = dr_config();
    config.dr_auto_sync.wait_store_timeout = astersql_pd_typeutil::Duration::from_nanos(100);
    let manager = ModeManager::new(
        config,
        Arc::new(StorageEndpoint::new(store)),
        paths.clone(),
        allocator,
        cancel.clone(),
    )
    .await
    .unwrap();
    (manager, leader, cancel, paths)
}

#[tokio::test]
async fn mcs_005_quorum_boundaries_and_repeated_ticks() {
    let now = 1_000_000;
    for (count, expected) in [(2, false), (3, false), (4, true)] {
        let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
        let (manager, leader, _, paths) = local_manager(backend.clone(), 510 + count).await;
        let mut rules = rules();
        rules[0].count = count as i64;
        // A learner and a non-global voter cannot turn a tie into a majority.
        let mut learner = rules[0].clone();
        learner.role = astersql_pd_schedule_placement::PeerRoleType::Learner;
        learner.count = 100;
        let mut range = rules[0].clone();
        range.start_key = vec![1];
        rules.extend([learner, range]);
        let infos: Vec<_> = stores(now)
            .into_iter()
            .rev()
            .map(|mut s| {
                if s.last_heartbeat == 0 {
                    s.last_heartbeat = now - 100;
                }
                Arc::new(StoreInfo::new(s))
            })
            .collect();
        assert_eq!(
            manager
                .tick_sync(&infos, &rules, now as i128)
                .await
                .unwrap(),
            expected
        );
        let state = manager.snapshot().await.dr_auto_sync;
        assert_eq!(state.state, if expected { "async_wait" } else { "sync" });
        assert_eq!(state.state_id, if expected { 2 } else { 1 });
        if expected {
            assert_eq!(state.available_stores, [100, 200, 300, 400]);
        }
        assert!(
            !manager
                .tick_sync(&infos, &rules, now as i128)
                .await
                .unwrap()
        );
        assert_eq!(manager.snapshot().await.dr_auto_sync, state);
        assert_eq!(
            StorageEndpoint::new(backend)
                .load_replication_status::<crate::DRAutoSyncStatus>(&paths, "dr-auto-sync")
                .await
                .unwrap(),
            Some(state)
        );
        leader.resign().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_005_write_failure_and_cancellation() {
    let fixture = crate::fixture::Fixture::start();
    let proxy = crate::mode_proxy_test::Proxy::start().await;
    proxy.target(
        fixture
            .endpoint
            .trim_start_matches("http://")
            .parse()
            .unwrap(),
    );
    let backend: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(
            &[format!("http://{}", proxy.address)],
            Duration::from_millis(500),
        )
        .await
        .unwrap(),
    );
    let (manager, leader, cancel, paths) = local_manager(backend.clone(), 520).await;
    let initial = manager.snapshot().await;
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    proxy.block(paths.replication_mode_path("dr-auto-sync").as_bytes());
    assert!(matches!(
        manager.tick_sync(&infos, &rules(), now as i128).await,
        Err(crate::ModeError::Storage(_))
    ));
    assert_eq!(manager.snapshot().await, initial);
    proxy.unblock();
    assert_eq!(
        StorageEndpoint::new(backend.clone())
            .load_replication_status::<crate::DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(initial.dr_auto_sync)
    );
    assert!(
        manager
            .tick_sync(&infos, &rules(), now as i128)
            .await
            .unwrap()
    );
    let saved = manager.snapshot().await;
    assert_eq!(
        saved.dr_auto_sync.state_id, 3,
        "failed publication consumed ID 2 without publishing it"
    );
    cancel.cancel();
    assert!(matches!(
        manager.tick_sync(&infos, &rules(), now as i128).await,
        Err(crate::ModeError::Cancelled)
    ));
    assert_eq!(manager.snapshot().await, saved);
    leader.resign().await.unwrap();
    proxy.stop().await;
}

#[tokio::test]
async fn mcs_005_liveness_removal_and_concurrent_publication() {
    let now = 1_000_000;
    // The down boundary is inclusive. Just inside the timeout, both DCs retain voters.
    for (offset, removed, expected) in [(99, false, false), (100, false, true), (100, true, false)]
    {
        let backend: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
        let (manager, leader, _, _) =
            local_manager(backend, 530 + offset as u64 + u64::from(removed)).await;
        let infos: Vec<_> = stores(now)
            .into_iter()
            .map(|mut s| {
                if s.last_heartbeat == 0 {
                    s.last_heartbeat = now - offset;
                }
                if removed && s.id == 700 {
                    s.node_state = NodeState::Removed;
                }
                Arc::new(StoreInfo::new(s))
            })
            .collect();
        let global_rules = rules();
        let (first, second) = tokio::join!(
            manager.tick_sync(&infos, &global_rules, now as i128),
            manager.tick_sync(&infos, &global_rules, now as i128)
        );
        assert_eq!(
            usize::from(first.unwrap()) + usize::from(second.unwrap()),
            usize::from(expected)
        );
        assert_eq!(
            manager.snapshot().await.dr_auto_sync.state_id,
            if expected { 2 } else { 1 }
        );
        leader.resign().await.unwrap();
    }
}
