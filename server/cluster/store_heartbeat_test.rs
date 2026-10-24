// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, CpuStats, KeyRange, NodeState, Peer, PeerStat, QueryStats, RegionInfo,
    RegionMeta, StoreInfo, StoreMeta, StoreState, StoreStats, TimeInterval,
};
use astersql_pd_statistics::HotStat;
use astersql_pd_statistics_utils::RWType;
use prometheus::Registry;
use std::sync::Arc;
use tokio::runtime::Handle;

const NOW: i64 = 1_000_000_000_000;
fn cluster() -> BasicCluster {
    let c = BasicCluster::new();
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    c
}
fn region(c: &mut BasicCluster, id: u64, store: u64) {
    let p = Peer {
        id: id + 10,
        store_id: store,
        ..Default::default()
    };
    c.regions.put_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            peers: vec![p.clone()],
            key_range: KeyRange {
                start_key: vec![id as u8],
                end_key: vec![id as u8 + 1],
            },
            ..Default::default()
        },
        Some(p),
        vec![],
        vec![],
    )));
}
fn report() -> StoreStats {
    StoreStats {
        store_id: 1,
        capacity: 100,
        available: 50,
        region_count: 1,
        interval: Some(TimeInterval {
            start_timestamp: 0,
            end_timestamp: 10,
        }),
        peer_stats: vec![PeerStat {
            region_id: 1,
            read_bytes: 9999998,
            read_keys: 9999999,
            query_stats: Some(QueryStats {
                get: 9999997,
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn hot() -> HotStat {
    HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap()
}
async fn degree(h: &HotStat, min: i64) -> usize {
    h.hot_cache
        .get_hot_peer_stats(RWType::READ, min)
        .await
        .unwrap()
        .get(&1)
        .map_or(0, Vec::len)
}

#[test]
fn state_task_41_store_heartbeat_existing_and_missing() {
    let mut c = cluster();
    let mut stats = report();
    stats.store_id = 42;
    assert_eq!(
        handle_store_heartbeat(&mut c, None, &stats, NOW)
            .unwrap_err()
            .to_string(),
        "store 42 not found"
    );
    assert_eq!(c.stores.store_count(), 1);
    stats.store_id = 1;
    let before = c.stores.get_store(1).unwrap();
    assert_eq!(
        handle_store_heartbeat(&mut c, None, &stats, NOW).unwrap(),
        NodeState::Serving
    );
    let after = c.stores.get_store(1).unwrap();
    assert_eq!(after.store_stats(), stats);
    assert_eq!(after.last_heartbeat_unix_nanos(), NOW);
    assert_eq!(before.last_heartbeat_unix_nanos(), 0);
    // Go clones share the raw stats holder, but detach heartbeat metadata.
    assert_eq!(before.store_stats(), stats);
    let reset = StoreStats {
        store_id: 1,
        ..Default::default()
    };
    handle_store_heartbeat(&mut c, None, &reset, NOW + 1).unwrap();
    assert_eq!(c.stores.get_store(1).unwrap().store_stats(), reset);
}

#[tokio::test]
async fn state_task_41_store_heartbeat_go_hot_cold_sequence() {
    let mut c = cluster();
    region(&mut c, 1, 1);
    let mut h = hot();
    let stats = report();
    for _ in 0..3 {
        handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    }
    assert_eq!(degree(&h, 3).await, 1);
    let p = h
        .hot_cache
        .get_hot_peer_stat(RWType::READ, 1, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        [p.get_load(0), p.get_load(1), p.get_load(2), p.get_load(3)],
        [1000000., 1000000., 1000000., 0.]
    );
    let cold = StoreStats {
        peer_stats: vec![],
        ..stats.clone()
    };
    handle_store_heartbeat(&mut c, Some(&h), &cold, NOW).unwrap();
    assert_eq!(degree(&h, 1).await, 0);
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    assert_eq!(degree(&h, 3).await, 1);
    for _ in 0..3 {
        handle_store_heartbeat(&mut c, Some(&h), &cold, NOW).unwrap();
    }
    assert_eq!(degree(&h, 0).await, 0);
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    assert_eq!(degree(&h, 1).await, 0);
    assert_eq!(degree(&h, 3).await, 0);
    for _ in 0..2 {
        handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    }
    assert_eq!(degree(&h, 3).await, 1);
    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_41_store_heartbeat_cpu_query_and_filters() {
    let mut c = cluster();
    region(&mut c, 1, 1);
    region(&mut c, 2, 2);
    let mut h = hot();
    let mut stats = report();
    stats.peer_stats[0].cpu_stats = Some(CpuStats {
        unified_read: 7,
        scheduler: 999,
    });
    stats.peer_stats[0].query_stats.as_mut().unwrap().scan = 2;
    stats.peer_stats[0]
        .query_stats
        .as_mut()
        .unwrap()
        .coprocessor = 3;
    stats.peer_stats[0].query_stats.as_mut().unwrap().gc = 100;
    let mut missing_peer = stats.peer_stats[0].clone();
    missing_peer.region_id = 2;
    let mut missing_region = stats.peer_stats[0].clone();
    missing_region.region_id = 99;
    stats.peer_stats.extend([missing_peer, missing_region]);
    for _ in 0..3 {
        handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    }
    let p = h
        .hot_cache
        .get_hot_peer_stat(RWType::READ, 1, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.get_load(2), 1000000.);
    assert_eq!(p.get_load(3), 7.);
    assert!(
        h.hot_cache
            .get_hot_peer_stat(RWType::READ, 2, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        h.hot_cache
            .get_hot_peer_stat(RWType::READ, 99, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        h.hot_cache
            .get_hot_peer_stat(RWType::WRITE, 1, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(h.stores_stats.get_rolling_store_stats(1).is_some());
    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_41_store_heartbeat_removed_and_unhealthy() {
    let mut c = cluster();
    let mut h = hot();
    let stats = report();
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    assert!(h.stores_stats.get_rolling_store_stats(1).is_some());
    c.stores.patch_store(1, |s| {
        s.edit_meta(|m| {
            m.state = StoreState::Tombstone;
            m.node_state = NodeState::Removed;
        })
    });
    let before = c.stores.get_store(1).unwrap();
    let changed = StoreStats {
        capacity: 9999,
        ..stats
    };
    handle_store_heartbeat(&mut c, Some(&h), &changed, NOW + 1).unwrap();
    assert_eq!(
        c.stores.get_store(1).unwrap().store_stats(),
        before.store_stats()
    );
    assert_eq!(
        c.stores.get_store(1).unwrap().last_heartbeat_unix_nanos(),
        NOW
    );
    assert!(h.stores_stats.get_rolling_store_stats(1).is_none());
    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_41_store_heartbeat_intervals_and_independent() {
    let mut c = cluster();
    region(&mut c, 1, 1);
    let mut h = hot();
    let mut stats = report();
    handle_store_heartbeat(&mut c, None, &stats, NOW).unwrap();
    assert!(h.stores_stats.get_rolling_store_stats(1).is_none());
    assert_eq!(degree(&h, 0).await, 0);
    stats.interval = None;
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    assert_eq!(degree(&h, 0).await, 0);
    stats.interval = Some(TimeInterval {
        start_timestamp: 10,
        end_timestamp: 0,
    });
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    // Wrapping interval exceeds the canonical hot-cache maximum and is rejected.
    assert_eq!(degree(&h, 0).await, 0);
    h.shutdown().await.unwrap();
}

#[test]
fn state_task_41_store_heartbeat_raw_oracle_rates() {
    use astersql_pd_statistics::{HotPeerCache, HotTask};
    let mut c = cluster();
    region(&mut c, 1, 1);
    let region = c.regions.get_region(1).unwrap();
    for cpu in [false, true] {
        let mut stats = report();
        if cpu {
            stats.peer_stats[0].cpu_stats = Some(CpuStats {
                unified_read: 7,
                scheduler: 999,
            });
            let q = stats.peer_stats[0].query_stats.as_mut().unwrap();
            q.scan = 2;
            q.coprocessor = 3;
            q.gc = 100;
        }
        let loads = super::store_heartbeat::peer_loads(&stats.peer_stats[0], 10);
        assert_eq!(
            loads,
            [
                9999998.,
                9999999.,
                if cpu { 10000002. } else { 9999997. },
                0.,
                0.,
                0.,
                if cpu { 70. } else { 0. },
                0.
            ]
        );
        let mut cache = HotPeerCache::new(RWType::READ);
        for _ in 0..3 {
            cache
                .run_task(HotTask::read_peer(&region, 1, loads, 10), NOW)
                .unwrap();
        }
        assert_eq!(
            cache.get(1, 1).unwrap().loads,
            [
                999999.8,
                999999.9,
                if cpu { 1000000.2 } else { 999999.7 },
                if cpu { 7. } else { 0. }
            ]
        );
    }
}

#[tokio::test]
async fn state_task_41_store_heartbeat_reported_missing_peer_is_not_cold() {
    let mut c = cluster();
    region(&mut c, 1, 1);
    let mut h = hot();
    let stats = report();
    for _ in 0..3 {
        handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    }
    assert_eq!(degree(&h, 3).await, 1);
    // Still reported, but no longer a peer on this store: skip read input and
    // preserve reported membership, matching the Go missing-peer branch.
    region(&mut c, 1, 2);
    handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap();
    assert_eq!(degree(&h, 3).await, 1);
    let cold = StoreStats {
        peer_stats: vec![],
        ..stats
    };
    handle_store_heartbeat(&mut c, Some(&h), &cold, NOW).unwrap();
    assert_eq!(degree(&h, 1).await, 0);
    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_41_store_heartbeat_closed_queue_keeps_core_update() {
    let mut c = cluster();
    region(&mut c, 1, 1);
    let mut h = hot();
    h.shutdown().await.unwrap();
    let stats = report();
    assert_eq!(
        handle_store_heartbeat(&mut c, Some(&h), &stats, NOW).unwrap(),
        NodeState::Serving
    );
    assert_eq!(c.stores.get_store(1).unwrap().store_stats(), stats);
    assert_eq!(
        c.stores.get_store(1).unwrap().last_heartbeat_unix_nanos(),
        NOW
    );
    h.shutdown().await.unwrap();
}
