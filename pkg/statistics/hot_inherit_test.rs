// Copyright 2026 AsterSQL.
use crate::{HotPeerCache, HotPeerStat};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::{ActionType, RWType};

fn region(stores: &[u64], leader: u64) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&store_id| Peer {
            id: store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let lead = peers.iter().find(|p| p.store_id == leader).cloned();
    RegionInfo::new(
        RegionMeta {
            id: 100,
            peers,
            ..Default::default()
        },
        lead,
        vec![],
        vec![],
    )
}
fn report(
    c: &HotPeerCache,
    k: RWType,
    r: &RegionInfo,
    peers: Option<&[u64]>,
    seconds: u64,
    rate: f64,
    now: i64,
) -> Vec<HotPeerStat> {
    let mut delta = [0.; 8];
    for dim in k.region_stats() {
        delta[dim.as_i32() as usize] = rate * seconds as f64;
    }
    c.check_peer_flow_at(r, peers, &delta, seconds, now, |_| [1.; 4])
}
fn apply(c: &mut HotPeerCache, items: Vec<HotPeerStat>) {
    for item in items {
        c.update_stat(item);
    }
}
fn warm(c: &mut HotPeerCache, k: RWType) {
    for _ in 0..200 {
        let items = report(
            c,
            k,
            &region(&[1, 2, 3], 1),
            None,
            k.report_interval(),
            100.,
            0,
        );
        apply(c, items);
    }
}
#[test]
fn state_task_21_hot_inherit_lookup_and_final_miss() {
    let mut c = HotPeerCache::new(RWType::READ);
    c.update_stat(HotPeerStat {
        region_id: 100,
        store_id: 1,
        action_type: ActionType::ADD,
        allow_inherited: true,
        ..Default::default()
    });
    let r = region(&[2], 2);
    let (old, inherit) = c.find_old_hot_peer_for_inheritance(&r);
    assert!(inherit);
    assert_eq!(old.unwrap().store_id, 1);
    let mut item = c.get(100, 1).unwrap().clone();
    item.allow_inherited = false;
    c.update_stat(item);
    let (old, inherit) = c.find_old_hot_peer_for_inheritance(&region(&[1], 1));
    assert!(!inherit);
    assert_eq!(old.unwrap().store_id, 1);
    assert!(
        c.find_old_hot_peer_for_inheritance(&region(&[2, 3, 2], 2))
            .0
            .is_none()
    );
}
#[test]
fn state_task_21_hot_inherit_replacement_preserves_window_and_isolation() {
    let k = RWType::READ;
    let mut c = HotPeerCache::new(k);
    warm(&mut c, k);
    let r = region(&[1, 3, 10], 1);
    let expired = c.collect_expired_items(&r);
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].store_id, 2);
    assert_eq!(expired[0].action_type, ActionType::REMOVE);
    let old_degree = c.get(100, 1).unwrap().hot_degree;
    let items = report(&c, k, &r, Some(&[10]), 4, 0., 5);
    assert_eq!(items[0].action_type, ActionType::UPDATE);
    assert_eq!(items[0].hot_degree, old_degree);
    assert_eq!(items[0].get_load(0), 100.);
    assert_eq!(items[0].interval_sum(), 4_000_000_000);
    assert!(!items[0].allow_inherited);
    apply(&mut c, items);
    apply(&mut c, expired);
    assert!(c.get(100, 2).is_none());
    assert_eq!(c.get(100, 1).unwrap().interval_sum(), 0);
    let items = report(&c, k, &r, Some(&[10]), 6, 100., 6);
    apply(&mut c, items);
    assert!(c.get(100, 10).unwrap().allow_inherited);
    assert_eq!(c.get(100, 1).unwrap().hot_degree, old_degree);
    assert_eq!(c.get(100, 10).unwrap().stores, vec![1, 3, 10]);
}
#[test]
fn state_task_21_hot_inherit_leader_read_write_cooldown() {
    for k in [RWType::READ, RWType::WRITE] {
        let mut c = HotPeerCache::new(k);
        warm(&mut c, k);
        let old_degree = c.get(100, 1).unwrap().hot_degree;
        let r = region(&[1, 2, 3], 2);
        let now = 1_000_000_000_000;
        let items = report(&c, k, &r, None, k.report_interval(), 0., now);
        assert_eq!(items.len(), 3);
        for item in &items {
            assert_eq!(item.last_transfer_leader_time, Some(now));
            assert_eq!(item.is_leader, item.store_id == 2);
            assert_eq!(
                item.hot_degree,
                old_degree - if k == RWType::READ { 1 } else { 0 }
            );
            assert!(item.needs_cooldown_transfer_leader(now, 3, k));
            assert!(!item.needs_cooldown_transfer_leader(
                now + 3 * k.report_interval() as i64 * 1_000_000_000,
                3,
                k
            ));
        }
        apply(&mut c, items);
        let items = report(&c, k, &r, None, k.report_interval(), 100., now + 1);
        for item in &items {
            assert_eq!(item.last_transfer_leader_time, Some(now));
        }
        apply(&mut c, items);
    }
}
#[test]
fn state_task_21_hot_inherit_partial_clone_resets_delta() {
    let k = RWType::READ;
    let mut c = HotPeerCache::new(k);
    let items = report(&c, k, &region(&[1], 1), None, 10, 100., 0);
    apply(&mut c, items);
    let items = report(&c, k, &region(&[1], 1), None, 4, 100., 0);
    apply(&mut c, items);
    let items = report(&c, k, &region(&[1, 2], 1), Some(&[2]), 6, 0., 0);
    // Go inheritance clones accumulated seconds but resets pending delta.
    assert_eq!(items[0].hot_degree, 0);
    assert_eq!(items[0].anti_count, 11);
    assert_eq!(c.get(100, 1).unwrap().interval_sum(), 4_000_000_000);
}
#[test]
fn state_task_21_hot_inherit_cache_inherit_new_flow() {
    let k = RWType::READ;
    let mut c = HotPeerCache::new(k);
    warm(&mut c, k);
    for rate in [100., 1000.] {
        let base = region(&[1, 2, 3], 1);
        let expired = c.collect_expired_items(&base);
        apply(&mut c, expired);
        for _ in 0..200 {
            let items = report(&c, k, &base, None, 10, rate, 0);
            apply(&mut c, items);
        }
        let r = region(&[1, 3, 10], 1);
        let items = report(&c, k, &r, None, 10, rate, 0);
        for item in &items {
            assert_eq!(item.loads[0], rate);
            assert_eq!(item.get_load(0), rate);
        }
        apply(&mut c, items);
    }
}

#[test]
fn state_task_21_hot_inherit_cold_peer_reentry_and_replica_changes() {
    for rate in [0., 100.] {
        let k = RWType::READ;
        let mut c = HotPeerCache::new(k);
        warm(&mut c, k);
        let mut remove = c.get(100, 2).unwrap().clone();
        remove.action_type = ActionType::REMOVE;
        c.update_stat(remove);
        let items = report(&c, k, &region(&[1, 2, 3], 1), Some(&[2]), 10, rate, 99);
        assert_eq!(items.len(), 1);
        if rate == 0. {
            assert_eq!(items[0].action_type, ActionType::REMOVE);
        } else {
            assert_eq!(items[0].hot_degree, 1);
            assert_eq!(items[0].anti_count, 12);
        }
        assert_eq!(items[0].last_transfer_leader_time, None);
    }
    let k = RWType::READ;
    let mut c = HotPeerCache::new(k);
    warm(&mut c, k);
    for stores in [&[1, 2, 3, 10][..], &[1, 3, 10][..], &[1, 3][..]] {
        let r = region(stores, 1);
        let expired = c.collect_expired_items(&r);
        let items = report(&c, k, &r, None, 10, 100., 99);
        for item in &items {
            assert_eq!(item.last_transfer_leader_time, None);
        }
        apply(&mut c, expired);
        apply(&mut c, items);
    }
}

#[test]
fn state_task_21_hot_inherit_write_skip_keeps_pending_window_and_dynamic_clock() {
    let k = RWType::WRITE;
    let mut c = HotPeerCache::new(k);
    warm(&mut c, k);
    let items = report(&c, k, &region(&[1, 2, 3], 1), None, 4, 100., 0);
    apply(&mut c, items);
    let degree = c.get(100, 1).unwrap().hot_degree;
    let changed = region(&[1, 2, 3], 2);
    let items = c.check_peer_flow_dynamic(&changed, None, &[0.; 8], 60, 123);
    for item in &items {
        assert_eq!(item.interval_sum(), 4_000_000_000);
        assert_eq!(item.hot_degree, degree);
        assert_eq!(item.get_load(0), 100.);
        assert_eq!(item.last_transfer_leader_time, Some(123));
    }
    // Dropping actions does not mutate the cached leader or pending delta.
    assert!(c.get(100, 1).unwrap().is_leader);
    assert_eq!(c.get(100, 1).unwrap().last_transfer_leader_time, None);
    apply(&mut c, items);
    let items = report(&c, k, &changed, None, 56, 100., 124);
    for item in &items {
        assert_eq!(item.hot_degree, degree + 1);
        assert_eq!(item.interval_sum(), 0);
    }
}
