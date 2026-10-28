// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta, StoreState};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::LabelStatistics;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn task_012_behavior() {
    let fixture = effects_fixture::Fixture::start();
    let storage = Arc::new(StorageEndpoint::new(Arc::new(fixture.store().await)));
    let paths = ClusterPaths::new(120);
    let cluster = BasicCluster::new();
    for (id, node_state, state) in [
        (1, NodeState::Preparing, StoreState::Up),
        (2, NodeState::Removing, StoreState::Offline),
    ] {
        let meta = StoreMeta {
            id,
            node_state,
            state,
            last_heartbeat: 1_000_000_000,
            ..Default::default()
        };
        storage.save_store_meta(&paths, &meta).await.unwrap();
        cluster.stores.put_store(Arc::new(StoreInfo::new(meta)));
    }
    let state = Arc::new(Mutex::new(HeartbeatEffectsState {
        cluster,
        region_stats: None,
        label_stats: LabelStatistics::new(),
        config: ProviderSnapshot::<()>::default(),
        rules: RuleManager::new(storage.clone(), paths),
        affinity: None,
    }));
    let cancel = CancellationToken::new();
    let s = state.clone();
    let endpoint = storage.clone();
    let worker = StoreStateWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        Duration::from_millis(10),
        move || {
            let s = s.clone();
            let endpoint = endpoint.clone();
            async move {
                assert!(
                    check_store_states(&s, &endpoint, paths, 1_000_000_000, false)
                        .await
                        .is_empty()
                );
            }
        },
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    worker.stop().await.unwrap();
    assert_eq!(
        state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .meta()
            .unwrap()
            .node_state,
        NodeState::Serving,
        "a real tick must promote a preparing store"
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 2)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Removed
    );
    // Once joined, additional time cannot publish lifecycle writes.
    let before = storage.load_store_meta(&paths, 2).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(storage.load_store_meta(&paths, 2).await.unwrap(), before);
}

#[tokio::test]
async fn task_012_controlled_clock_and_join() {
    use astersql_pd_storage_kv::MemoryStore;
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(121);
    let cluster = BasicCluster::new();
    let meta = StoreMeta {
        id: 1,
        node_state: NodeState::Preparing,
        ..Default::default()
    };
    storage.save_store_meta(&paths, &meta).await.unwrap();
    cluster.stores.put_store(Arc::new(StoreInfo::new(meta)));
    let state = Arc::new(Mutex::new(HeartbeatEffectsState {
        cluster,
        region_stats: None,
        label_stats: LabelStatistics::new(),
        config: ProviderSnapshot::<()>::default(),
        rules: RuleManager::new(storage.clone(), paths),
        affinity: None,
    }));
    tokio::time::pause();
    let s = state.clone();
    let endpoint = storage.clone();
    let cancel = CancellationToken::new();
    let worker = StoreStateWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        Duration::from_secs(10),
        move || {
            let s = s.clone();
            let endpoint = endpoint.clone();
            async move {
                assert!(
                    check_store_states(&s, &endpoint, paths, 0, false)
                        .await
                        .is_empty()
                );
            }
        },
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(9)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    tokio::time::advance(Duration::from_millis(1001)).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::task::yield_now().await;
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    cancel.cancel();
    worker.stop().await.unwrap();
    state
        .lock()
        .unwrap()
        .cluster
        .stores
        .patch_store(1, |s| s.edit_meta(|m| m.node_state = NodeState::Preparing));
    tokio::time::advance(Duration::from_secs(100)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .meta()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
}

#[tokio::test]
async fn task_012_fault_retry_and_boundaries() {
    let mut fixture = effects_fixture::Fixture::start();
    let storage = Arc::new(StorageEndpoint::new(Arc::new(fixture.store().await)));
    let paths = ClusterPaths::new(122);
    let cluster = BasicCluster::new();
    let age = 30 * 24 * 3600 * 1_000_000_000_i128;
    for (id, node_state, state) in [
        (1, NodeState::Preparing, StoreState::Up),
        (2, NodeState::Removing, StoreState::Offline),
        (3, NodeState::Removing, StoreState::Offline),
        (4, NodeState::Removed, StoreState::Tombstone),
        (5, NodeState::Preparing, StoreState::Up),
    ] {
        let meta = StoreMeta {
            id,
            node_state,
            state,
            physically_destroyed: id == 5,
            ..Default::default()
        };
        storage.save_store_meta(&paths, &meta).await.unwrap();
        let mut store = StoreInfo::new(meta);
        if id == 3 {
            store.set_region_size(1);
        }
        cluster.stores.put_store(Arc::new(store));
    }
    let state = Arc::new(Mutex::new(HeartbeatEffectsState {
        cluster,
        region_stats: None,
        label_stats: LabelStatistics::new(),
        config: ProviderSnapshot::<()>::default(),
        rules: RuleManager::new(storage.clone(), paths),
        affinity: None,
    }));
    fixture.command("stop", "stopped");
    let errors = check_store_states(&state, &storage, paths, age + 1, false).await;
    assert_eq!(
        errors.len(),
        3,
        "save and delete errors must retain the old cache"
    );
    assert_eq!(
        state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .meta()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    assert!(state.lock().unwrap().cluster.stores.get_store(4).is_some());
    fixture.command("start", "restarted");
    let storage = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert!(
        check_store_states(&state, &storage, paths, age, false)
            .await
            .is_empty()
    );
    assert!(
        storage.load_store_meta(&paths, 4).await.unwrap().is_some(),
        "GC uses a strict greater-than boundary"
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 3)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Removing
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 5)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 2)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Removed
    );
    assert!(
        check_store_states(&state, &storage, paths, age + 1, false)
            .await
            .is_empty()
    );
    assert!(storage.load_store_meta(&paths, 4).await.unwrap().is_none());
    assert!(state.lock().unwrap().cluster.stores.get_store(4).is_none());
}

#[tokio::test]
async fn task_012_preparing_threshold_and_uptime() {
    use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
    use astersql_pd_storage_kv::MemoryStore;
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(123);
    let mut cluster = BasicCluster::new();
    for id in 1..=100_u64 {
        let mut region = RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        region.stats_mut().approximate_size = 10;
        cluster.regions.put_region(Arc::new(region));
    }
    let mut config = ProviderSnapshot::<()>::default();
    config.replication.enable_placement_rules = false;
    config.replication.max_replicas = 3;
    // The default unadjusted duration is zero; select a real nonzero host setting.
    config.schedule.max_store_preparing_time = "1h".parse().unwrap();
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Preparing,
        last_heartbeat: 1_000_000_000,
        ..Default::default()
    });
    store.set_region_size(0);
    storage
        .save_store_meta(&paths, store.meta().unwrap())
        .await
        .unwrap();
    cluster.stores.put_store(Arc::new(store));
    let state = Arc::new(Mutex::new(HeartbeatEffectsState {
        cluster,
        region_stats: None,
        label_stats: LabelStatistics::new(),
        config,
        rules: RuleManager::new(storage.clone(), paths),
        affinity: None,
    }));
    // 100 regions excludes the initial-cluster shortcut; no readiness means no promotion.
    assert!(
        check_store_states(&state, &storage, paths, 0, false)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    // An uninitialized placement manager must never look like zero required data.
    state
        .lock()
        .unwrap()
        .config
        .replication
        .enable_placement_rules = true;
    assert!(
        check_store_states(&state, &storage, paths, 0, true)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    let (config, stores) = {
        let state = state.lock().unwrap();
        (
            state.config.clone(),
            state
                .cluster
                .stores
                .get_stores()
                .into_iter()
                .map(|s| s.as_ref().clone())
                .collect::<Vec<_>>(),
        )
    };
    let mut rules = RuleManager::new(storage.clone(), paths);
    rules
        .initialize(3, &[], "", false, &config, &stores)
        .await
        .unwrap();
    state.lock().unwrap().rules = rules;
    assert!(
        check_store_states(&state, &storage, paths, 0, true)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    state
        .lock()
        .unwrap()
        .cluster
        .stores
        .patch_store(1, |s| s.set_region_size(10_000));
    assert!(
        check_store_states(&state, &storage, paths, 0, true)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    state
        .lock()
        .unwrap()
        .config
        .replication
        .enable_placement_rules = false;
    state.lock().unwrap().cluster.stores.patch_store(1, |s| {
        s.edit_meta(|m| m.node_state = NodeState::Preparing);
        s.set_region_size(0);
    });
    assert!(
        check_store_states(&state, &storage, paths, 0, true)
            .await
            .is_empty()
    );
    assert_eq!(
        state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(1)
            .unwrap()
            .meta()
            .unwrap()
            .node_state,
        NodeState::Preparing
    );
    state
        .lock()
        .unwrap()
        .cluster
        .stores
        .patch_store(1, |s| s.set_region_size(10_000));
    assert!(
        check_store_states(&state, &storage, paths, 0, true)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    // Elapsed wall time alone is insufficient: uptime is heartbeat minus start.
    state.lock().unwrap().cluster.stores.patch_store(1, |s| {
        s.edit_meta(|m| m.node_state = NodeState::Preparing);
        s.set_last_heartbeat_unix_nanos(3_600_000_000_000);
    });
    assert!(
        check_store_states(&state, &storage, paths, 0, false)
            .await
            .is_empty()
    );
    assert_eq!(
        storage
            .load_store_meta(&paths, 1)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
}
