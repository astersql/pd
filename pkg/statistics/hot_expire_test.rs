// Copyright 2026 AsterSQL.
use crate::{HotPeerCache, HotPeerStat};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::{ActionType, RWType};
use std::collections::BTreeSet;
const SECOND: i64 = 1_000_000_000;
fn set(ids: &[u64]) -> BTreeSet<u64> {
    ids.iter().copied().collect()
}
fn region(id: u64, stores: &[u64]) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&store_id| Peer {
            id: store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            id,
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    )
}
fn apply(c: &mut HotPeerCache, items: Vec<HotPeerStat>, now: i64) {
    for item in items {
        c.update_stat_at(item, now);
    }
}
fn flow(c: &HotPeerCache, k: RWType, r: &RegionInfo, interval: u64, rate: f64) -> Vec<HotPeerStat> {
    let mut delta = [0.; 8];
    for dim in k.region_stats() {
        delta[dim.as_i32() as usize] = rate * interval as f64;
    }
    c.check_peer_flow(r, None, &delta, interval, |_| [1.; 4])
}
fn warm(c: &mut HotPeerCache, k: RWType, r: &RegionInfo) {
    for _ in 0..200 {
        let items = flow(c, k, r, k.report_interval(), 100000.);
        apply(c, items, 0);
    }
}
#[test]
fn state_task_22_hot_expire_missing_report_partial_window_and_isolation() {
    let k = RWType::READ;
    let mut c = HotPeerCache::new(k);
    warm(&mut c, k, &region(100, &[1, 2]));
    warm(&mut c, k, &region(200, &[1]));
    assert!(c.check_cold_peer(1, &set(&[100]), 2, 0).is_empty());
    assert!(c.check_cold_peer(99, &set(&[]), 10, 0).is_empty());
    let degree = c.get(200, 1).unwrap().hot_degree;
    let items = c.check_cold_peer(1, &set(&[100]), 4, 0);
    assert_eq!(items.len(), 1);
    assert!(items[0].in_cold);
    assert_eq!(items[0].loads, vec![0.; 4]);
    assert_eq!(items[0].hot_degree, degree);
    assert_eq!(items[0].interval_sum(), 4 * SECOND);
    assert_eq!(c.get(200, 1).unwrap().interval_sum(), 0); // discarded actions are inert
    apply(&mut c, items, 0);
    let items = c.check_cold_peer(1, &set(&[100]), 6, 0);
    assert_eq!(items[0].hot_degree, degree - 1);
    assert_eq!(items[0].anti_count, 11);
    assert_eq!(items[0].interval_sum(), 0);
    apply(&mut c, items, 0);
    assert_eq!(c.get(100, 1).unwrap().hot_degree, 200);
    for _ in 0..11 {
        let items = c.check_cold_peer(1, &set(&[100]), 10, 0);
        apply(&mut c, items, 0);
    }
    assert!(c.get(200, 1).is_none());
    assert_eq!(c.regions_of_store(1), set(&[100]));
    assert_eq!(c.stores_of_region(100), set(&[1, 2]));
}
#[test]
fn state_task_22_hot_expire_ttl_refresh_boundary_and_rollback() {
    let mut c = HotPeerCache::new(RWType::WRITE);
    c.set_peer_ttl(100);
    let r = region(100, &[1, 2]);
    let items = flow(&c, RWType::WRITE, &r, 60, 100000.);
    apply(&mut c, items, 0);
    c.gc(99, &set(&[1, 2]));
    assert!(c.get(100, 1).is_some());
    let items = c.check_peer_flow(&r, Some(&[2]), &[6000000.; 8], 60, |_| [1.; 4]);
    apply(&mut c, items, 50);
    c.gc(-1, &set(&[1, 2]));
    assert!(c.get(100, 1).is_some());
    c.gc(100, &set(&[1, 2]));
    assert!(c.get(100, 1).is_none());
    assert!(c.get(100, 2).is_some());
    assert_eq!(c.stores_of_region(100), set(&[2]));
    assert!(c.regions_of_store(1).is_empty());
    c.gc(199, &set(&[1, 2]));
    assert!(c.get(100, 2).is_some()); // GC throttle
    c.gc(200, &set(&[1, 2]));
    assert!(c.get(100, 2).is_none());
    assert!(c.stores_of_region(100).is_empty());
}
#[test]
fn state_task_22_hot_expire_removed_store_and_threshold_only_store() {
    let mut c = HotPeerCache::new(RWType::WRITE);
    c.set_peer_ttl(100);
    for id in 1..=60 {
        let items = flow(&c, RWType::WRITE, &region(id, &[1, 2]), 60, 100000.);
        apply(&mut c, items, 0);
    }
    assert_eq!(c.calc_hot_thresholds(1, 0)[0], 80000.);
    c.calc_hot_thresholds(9, 0);
    for id in 1..=60 {
        let items = flow(&c, RWType::WRITE, &region(id, &[1, 2]), 60, 100000.);
        apply(&mut c, items, 50);
    }
    c.gc(100, &set(&[2]));
    assert_eq!(c.stores_of_region(1), set(&[2]));
    assert_eq!(c.regions_of_store(2).len(), 60);
    assert_eq!(c.calc_hot_thresholds(2, 101)[0], 80000.);
    assert!(c.regions_of_store(1).is_empty());
    // Removed store threshold must be purged even if refresh would otherwise retain it.
    assert_eq!(c.calc_hot_thresholds(1, 101), c.minimum_thresholds());
    assert!(c.hot_thresholds.snapshots.lock().unwrap().get(&9).is_none());
    assert!(c.get(1, 1).is_none());
}
#[test]
fn state_task_22_hot_expire_region_disappearance_and_threshold_recompute() {
    let mut c = HotPeerCache::new(RWType::WRITE);
    c.set_thresholds_update_interval(0);
    for id in 1..=60 {
        let items = flow(&c, RWType::WRITE, &region(id, &[1, 2]), 60, 100000.);
        apply(&mut c, items, 0);
    }
    assert_eq!(c.calc_hot_thresholds(1, 0)[0], 80000.);
    c.remove_region(1);
    c.remove_region(1);
    assert!(c.get(1, 1).is_none());
    assert!(c.stores_of_region(1).is_empty());
    assert_eq!(c.regions_of_store(1).len(), 59);
    assert_eq!(c.calc_hot_thresholds(1, 1), c.minimum_thresholds());
    assert!(c.get(2, 2).is_some());
}
#[test]
fn state_task_22_hot_expire_fixed_replica_moves_and_cold_reports() {
    for count in [3, 5] {
        for interval in [120, 60, 10, 5] {
            let k = RWType::WRITE;
            let mut c = HotPeerCache::new(k);
            let mut stores: Vec<_> = (1..=count).collect();
            warm(&mut c, k, &region(100, &stores));
            // Fixed replacement order, with different partial windows across peers.
            for i in 0..30 {
                if i % 5 == 0 {
                    stores[1] = if stores[1] == 2 { 10 } else { 2 };
                }
                let r = region(100, &stores);
                let expired = c.collect_expired_items(&r);
                let items = flow(&c, k, &r, interval, 100000.);
                apply(&mut c, items, 0);
                apply(&mut c, expired, 0);
            }
            assert_eq!(c.stores_of_region(100), stores.iter().copied().collect());
            for _ in 0..200 {
                let items = flow(&c, k, &region(100, &stores), interval, 0.);
                apply(&mut c, items, 0);
            }
            assert!(c.stores_of_region(100).is_empty());
            for store in stores {
                assert!(c.regions_of_store(store).is_empty());
                assert!(c.get(100, store).is_none());
            }
        }
    }
}
#[test]
fn state_task_22_hot_expire_cold_write_and_default_ttl() {
    for k in [RWType::READ, RWType::WRITE] {
        let mut c = HotPeerCache::new(k);
        warm(&mut c, k, &region(100, &[1]));
        for i in 0..k.default_anti_count() {
            let items = c.check_cold_peer(1, &set(&[]), k.report_interval(), i as i64);
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].anti_count,
                k.default_anti_count() as i64 - i as i64 - 1
            );
            assert_eq!(
                items[0].action_type,
                if i + 1 == k.default_anti_count() {
                    ActionType::REMOVE
                } else {
                    ActionType::UPDATE
                }
            );
            apply(&mut c, items, i as i64);
        }
        assert!(c.get(100, 1).is_none());
        warm(&mut c, k, &region(100, &[1]));
        let ttl = 3 * k.report_interval() as i64 * SECOND;
        c.gc(ttl - 1, &set(&[1]));
        assert!(c.get(100, 1).is_some());
        c.gc(ttl, &set(&[1]));
        assert!(c.get(100, 1).is_none());
    }
}
