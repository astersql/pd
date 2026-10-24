// Copyright 2026 AsterSQL.
use crate::{StoreStatInformer, StoresStats};
use astersql_pd_core::{
    BasicCluster, RecordPair, StoreInfo, StoreMeta, StoreState, StoreStats, TimeInterval,
};
use astersql_pd_statistics_utils::StoreLoadKind as K;
use std::sync::Arc;
const NOW: i64 = 100_000_000_000_000;
fn report() -> StoreStats {
    StoreStats {
        interval: Some(TimeInterval {
            start_timestamp: 0,
            end_timestamp: 10,
        }),
        bytes_read: 100,
        cpu_usages: vec![RecordPair {
            key: "unified-read-pool-0".into(),
            value: 70,
        }],
        ..Default::default()
    }
}
fn populate(stats: &StoresStats, cluster: &BasicCluster, count: u64) {
    for id in 1..=count {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            last_heartbeat: NOW,
            ..Default::default()
        })));
        stats.observe(id, &StoreStats::default());
    }
}
#[test]
fn state_task_11_stores_go_filter_unhealthy() {
    let stats = StoresStats::new();
    let cluster = BasicCluster::new();
    populate(&stats, &cluster, 5);
    assert_eq!(stats.get_stores_loads().len(), 5);
    cluster.stores.patch_store(1, |s| {
        s.set_last_heartbeat_unix_nanos(NOW - 86_400_000_000_000)
    });
    cluster
        .stores
        .patch_store(2, |s| s.set_store_state(StoreState::Tombstone, None));
    cluster.stores.delete_store(3);
    stats.filter_unhealthy_store(&cluster.stores, NOW as i128);
    let mut ids: Vec<_> = stats.get_stores_loads().into_keys().collect();
    ids.sort();
    assert_eq!(ids, [4, 5]);
}
#[test]
fn state_task_11_stores_health_boundary_recovery_and_empty() {
    let stats = StoresStats::default();
    let cluster = BasicCluster::new();
    stats.filter_unhealthy_store(&cluster.stores, NOW as i128);
    assert!(stats.get_stores_loads().is_empty());
    populate(&stats, &cluster, 4);
    cluster.stores.patch_store(1, |s| {
        s.set_last_heartbeat_unix_nanos(NOW - 600_000_000_000)
    });
    cluster.stores.patch_store(2, |s| {
        s.set_last_heartbeat_unix_nanos(NOW - 600_000_000_001)
    });
    cluster
        .stores
        .patch_store(3, |s| s.set_store_state(StoreState::Offline, Some(true)));
    cluster
        .stores
        .patch_store(4, |s| s.set_store_state(StoreState::Offline, Some(false)));
    stats.set(2, &report());
    stats.filter_unhealthy_store(&cluster.stores, NOW as i128);
    let mut ids: Vec<_> = stats.get_stores_loads().into_keys().collect();
    ids.sort();
    assert_eq!(ids, [1, 4]);
    cluster
        .stores
        .patch_store(2, |s| s.set_last_heartbeat_unix_nanos(NOW));
    stats.filter_unhealthy_store(&cluster.stores, NOW as i128);
    assert!(stats.get_rolling_store_stats(2).is_none());
    stats.observe(2, &StoreStats::default());
    assert_eq!(stats.get_store_read_cpu_recent_max(2), 0.);
}
#[test]
fn state_task_11_stores_creation_deletion_and_snapshots() {
    let stats = StoresStats::new();
    assert!(stats.get_rolling_store_stats(9).is_none());
    assert_eq!(stats.get_store_read_cpu_recent_max(9), 0.);
    stats.get_or_create_rolling_store_stats(9);
    stats.set(9, &report());
    stats.get_or_create_rolling_store_stats(9);
    stats.set_regions_stats(&[(9, 30., 40.)]);
    let informer: &dyn StoreStatInformer = &stats;
    let old = informer.get_stores_loads();
    let mut detached = stats.get_rolling_store_stats(9).unwrap();
    detached.set_regions_stats(900., 800.);
    let mut changed = informer.get_stores_loads();
    changed.get_mut(&9).unwrap()[0] = 999.;
    assert_eq!(stats.get_stores_loads(), old);
    stats.observe_regions_stats(&[(9, 10., 20.), (10, 50., 60.)]);
    assert_eq!(
        stats.get_stores_loads()[&9][K::REGIONS_WRITE_BYTES.as_i32() as usize],
        20.
    );
    assert_eq!(old[&9][K::READ_BYTES.as_i32() as usize], 10.);
    assert_eq!(old[&9][K::REGIONS_WRITE_BYTES.as_i32() as usize], 30.);
    stats.remove_rolling_store_stats(9);
    stats.remove_rolling_store_stats(9);
    assert_eq!(detached.get_load(K::READ_BYTES), 10.);
    assert_eq!(stats.get_store_read_cpu_recent_max(9), 0.);
    assert!(stats.get_rolling_store_stats(9).is_none());
    assert_eq!(
        stats
            .get_or_create_rolling_store_stats(9)
            .get_load(K::READ_BYTES),
        0.
    );
    stats.observe(9, &report());
    assert_eq!(stats.get_store_read_cpu_recent_max(9), 70.);
    assert_eq!(old.len(), 1);
}
#[test]
fn state_task_11_stores_concurrent_snapshots_and_removal() {
    let stats = Arc::new(StoresStats::new());
    std::thread::scope(|scope| {
        for id in 0..4 {
            let stats = stats.clone();
            scope.spawn(move || {
                for _ in 0..100 {
                    stats.set(id, &report());
                    let snapshot = stats.get_stores_loads();
                    stats.remove_rolling_store_stats(id);
                    for loads in snapshot.values() {
                        assert_eq!(loads[K::READ_BYTES.as_i32() as usize], 10.);
                    }
                }
            });
        }
    });
    assert!(stats.get_stores_loads().is_empty());
}
