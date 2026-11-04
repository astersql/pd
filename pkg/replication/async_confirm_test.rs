// Copyright 2026 AsterSQL.
use crate::StoreDRStatus;
use crate::async_wait_test::{go_wait_scenario, local_manager, rules, stores};
use astersql_pd_core::StoreInfo;
use astersql_pd_proto::{
    pdpb,
    replication_modepb::{DrAutoSyncState, StoreDrAutoSyncStatus},
};
use astersql_pd_storage_domain::{Consistency, MemoryStore, MetaStore, Read};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub(crate) async fn go_async_scenario(
    metas: &[astersql_pd_core::StoreMeta],
) -> (
    astersql_pd_compat_tests::harness::Server,
    Arc<dyn MetaStore>,
    astersql_pd_keypath::ClusterPaths,
    Value,
) {
    let (server, backend, paths, waiting) = go_wait_scenario(metas, rules(), "async_wait").await;
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
    assert_eq!(
        crate::mode_state_test::post(
            &server,
            &json!({"dr-auto-sync":{"pause-region-split":"true"}})
        )
        .0,
        200
    );
    let mut observations = vec![];
    // Wrong state/ID and partial acknowledgments must not authorize async.
    for (store_id, state, state_id) in [
        (100, DrAutoSyncState::AsyncWait, 1),
        (200, DrAutoSyncState::Sync, 2),
        (300, DrAutoSyncState::AsyncWait, 2),
        (300, DrAutoSyncState::AsyncWait, 2),
        (400, DrAutoSyncState::AsyncWait, 2),
    ] {
        let response = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: id,
                    ..Default::default()
                }),
                stats: Some(pdpb::StoreStats {
                    store_id,
                    capacity: 1_000_000_000,
                    available: 900_000_000,
                    used_size: 100_000_000,
                    ..Default::default()
                }),
                dr_autosync_status: Some(StoreDrAutoSyncStatus {
                    state: state as i32,
                    state_id,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        assert!(
            response
                .replication_status
                .unwrap()
                .dr_auto_sync
                .unwrap()
                .pause_region_split
        );
        observations.push(json!({"store_id":store_id,"state":state as i32,"state_id":state_id}));
    }
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        server.get("/pd/api/v1/replication_mode/status").unwrap().1,
        waiting["http"]
    );
    let confirm_started = chrono::Utc::now();
    for store_id in [100, 200] {
        let response = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: id,
                    ..Default::default()
                }),
                stats: Some(pdpb::StoreStats {
                    store_id,
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
        assert!(response.header.unwrap().error.is_none());
    }
    let go = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row = backend
                .read(Read::prefix(
                    paths.replication_mode_path("dr-auto-sync").as_bytes(),
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            let value: Value = serde_json::from_slice(&row.entries[0].value).unwrap();
            if value["state"] == "async" {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(go["state_id"], 3);
    let go_time =
        chrono::DateTime::parse_from_rfc3339(go["async_start"].as_str().unwrap()).unwrap();
    assert!(go_time >= confirm_started && go_time <= chrono::Utc::now());
    println!(
        "Go acknowledgments: {}; persisted async: {}",
        json!(observations),
        go
    );
    (server, backend, paths, go)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_006_behavior() {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let metas = stores(now);
    let (mut server, _, _, go) = go_async_scenario(&metas).await;
    server.stop().unwrap();
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, _) = local_manager(store, 606).await;
    let mut config = manager.snapshot().await.config;
    config.dr_auto_sync.wait_store_timeout =
        astersql_pd_typeutil::Duration::from_nanos(3_600_000_000_000);
    config.dr_auto_sync.pause_region_split = true;
    manager.update_config(config).await.unwrap();
    let infos: Vec<_> = metas
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    assert!(
        manager
            .tick_sync(&infos, &rules(), now as i128)
            .await
            .unwrap()
    );
    for id in [100, 200, 300, 400] {
        manager
            .update_store_dr_status(
                id,
                StoreDRStatus {
                    state: "async_wait".into(),
                    state_id: 2,
                },
            )
            .await;
    }
    let rust_confirm_started = chrono::Utc::now();
    assert!(
        manager
            .tick_async_wait(&infos, &rules(), now as i128)
            .await
            .unwrap(),
        "all four real acknowledgments must publish async like Go"
    );
    let mut actual = serde_json::to_value(manager.snapshot().await.dr_auto_sync).unwrap();
    let rust_time =
        chrono::DateTime::parse_from_rfc3339(actual["async_start"].as_str().unwrap()).unwrap();
    assert!(rust_time >= rust_confirm_started && rust_time <= chrono::Utc::now());
    assert!(manager.is_region_split_paused().await);
    actual["async_start"] = go["async_start"].clone();
    assert_eq!(actual, go);
    leader.resign().await.unwrap();
}

#[tokio::test]
async fn mcs_006_partial_stale_duplicate_and_elapsed_wait() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, _) = local_manager(store, 607).await;
    let now = 1_000_000;
    let infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    let rules = rules();
    let mut config = manager.snapshot().await.config;
    config.dr_auto_sync.pause_region_split = true;
    manager.update_config(config).await.unwrap();
    assert!(!manager.is_region_split_paused().await);
    manager
        .tick_sync(&infos, &rules, now as i128)
        .await
        .unwrap();
    assert!(manager.is_region_split_paused().await);
    let waiting = manager.snapshot().await;
    for (id, state, state_id) in [
        (100, "async_wait", 1),
        (200, "sync", 2),
        (300, "async_wait", 2),
        (300, "async_wait", 2),
        (400, "async_wait", 2),
    ] {
        manager
            .update_store_dr_status(
                id,
                StoreDRStatus {
                    state: state.into(),
                    state_id,
                },
            )
            .await;
    }
    assert!(
        !manager
            .tick_async_wait(&infos, &rules, now as i128 + 1_000_000_000)
            .await
            .unwrap()
    );
    assert_eq!(
        manager.snapshot().await,
        waiting,
        "elapsed wait is never an acknowledgment"
    );
    manager
        .update_store_dr_status(
            100,
            StoreDRStatus {
                state: "async_wait".into(),
                state_id: 2,
            },
        )
        .await;
    manager
        .update_store_dr_status(
            200,
            StoreDRStatus {
                state: "async_wait".into(),
                state_id: 2,
            },
        )
        .await;
    // Go's last report wins, even if its state ID regresses.
    manager
        .update_store_dr_status(
            100,
            StoreDRStatus {
                state: "async_wait".into(),
                state_id: 1,
            },
        )
        .await;
    assert!(
        !manager
            .tick_async_wait(&infos, &rules, now as i128)
            .await
            .unwrap()
    );
    manager
        .update_store_dr_status(
            100,
            StoreDRStatus {
                state: "async_wait".into(),
                state_id: 2,
            },
        )
        .await;
    assert!(
        manager
            .tick_async_wait(&infos, &rules, now as i128)
            .await
            .unwrap()
    );
    let published = manager.snapshot().await;
    assert_eq!(published.dr_auto_sync.state_id, 3);
    assert!(published.dr_auto_sync.async_start.is_some());
    assert!(
        !manager
            .tick_async_wait(&infos, &rules, now as i128)
            .await
            .unwrap()
    );
    assert_eq!(manager.snapshot().await, published);
    leader.resign().await.unwrap();
}

#[tokio::test]
async fn mcs_006_available_set_and_sync_recovery_priority() {
    let store: Arc<dyn MetaStore> = Arc::new(MemoryStore::default());
    let (manager, leader, _, _) = local_manager(store, 608).await;
    let now = 1_000_000;
    let mut infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    manager
        .tick_sync(&infos, &rules(), now as i128)
        .await
        .unwrap();
    for id in [100, 200, 300, 400] {
        manager
            .update_store_dr_status(
                id,
                StoreDRStatus {
                    state: "async_wait".into(),
                    state_id: 2,
                },
            )
            .await;
    }
    Arc::make_mut(&mut infos[0]).set_last_heartbeat_unix_nanos(0);
    assert!(
        manager
            .tick_async_wait(&infos, &rules(), now as i128)
            .await
            .unwrap()
    );
    let changed = manager.snapshot().await;
    assert_eq!(changed.dr_auto_sync.state, "async_wait");
    assert_eq!(changed.dr_auto_sync.state_id, 3);
    assert_eq!(changed.dr_auto_sync.available_stores, [200, 300, 400]);
    assert!(
        !manager
            .tick_async_wait(&infos, &rules(), now as i128)
            .await
            .unwrap(),
        "old acknowledgments must not authorize the new ID"
    );
    // Restore DR liveness. Sync takes priority even if no primary confirmed ID 3.
    Arc::make_mut(&mut infos[6]).set_last_heartbeat_unix_nanos(now);
    assert!(
        manager
            .tick_async_wait(&infos, &rules(), now as i128)
            .await
            .unwrap()
    );
    let synced = manager.snapshot().await.dr_auto_sync;
    assert_eq!(synced.state, "sync");
    assert_eq!(synced.state_id, 4);
    assert!(synced.available_stores.is_empty());
    assert!(synced.async_start.is_none());
    assert!(!manager.is_region_split_paused().await);
    leader.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_006_real_write_failure_and_cancel() {
    use astersql_pd_storage_domain::StorageEndpoint;
    use astersql_pd_storage_kv::EtcdMetaStore;
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
    let (manager, leader, cancel, paths) = local_manager(backend.clone(), 609).await;
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap();
    let infos: Vec<_> = stores(now)
        .into_iter()
        .map(|s| Arc::new(StoreInfo::new(s)))
        .collect();
    manager
        .tick_sync(&infos, &rules(), now as i128)
        .await
        .unwrap();
    for id in [100, 200, 300, 400] {
        manager
            .update_store_dr_status(
                id,
                StoreDRStatus {
                    state: "async_wait".into(),
                    state_id: 2,
                },
            )
            .await;
    }
    let waiting = manager.snapshot().await;
    proxy.block(paths.replication_mode_path("dr-auto-sync").as_bytes());
    assert!(matches!(
        manager.tick_async_wait(&infos, &rules(), now as i128).await,
        Err(crate::ModeError::Storage(_))
    ));
    assert_eq!(manager.snapshot().await, waiting);
    proxy.unblock();
    let endpoint = StorageEndpoint::new(backend);
    assert_eq!(
        endpoint
            .load_replication_status::<crate::DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(waiting.dr_auto_sync)
    );
    assert!(
        manager
            .tick_async_wait(&infos, &rules(), now as i128)
            .await
            .unwrap()
    );
    let saved = manager.snapshot().await;
    assert_eq!(saved.dr_auto_sync.state_id, 4);
    assert_eq!(
        endpoint
            .load_replication_status::<crate::DRAutoSyncStatus>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(saved.dr_auto_sync.clone())
    );
    cancel.cancel();
    assert!(matches!(
        manager.tick_async_wait(&infos, &rules(), now as i128).await,
        Err(crate::ModeError::Cancelled)
    ));
    assert_eq!(manager.snapshot().await, saved);
    leader.resign().await.unwrap();
    proxy.stop().await;
}
