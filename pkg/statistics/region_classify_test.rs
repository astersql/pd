// Copyright 2026 AsterSQL.
use crate::{RegionStatisticType::*, RegionStatistics};
use astersql_pd_core::{
    DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreState,
};
use astersql_pd_schedule_config::ProviderSnapshot;
fn config() -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.replication.max_replicas = 3;
    c.replication.enable_placement_rules = false;
    c.schedule.max_merge_region_size = 20;
    c.schedule.max_merge_region_keys = 200000;
    c
}
fn region(
    id: u64,
    n: usize,
    learner: bool,
    down: usize,
    pending: usize,
    size: i64,
    keys: i64,
    witness: bool,
) -> RegionInfo {
    let peers: Vec<_> = (1..=n)
        .map(|i| Peer {
            id: i as u64,
            store_id: i as u64,
            role: if learner && i == n {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: witness && i == 1,
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        peers
            .iter()
            .take(down)
            .map(|p| DownPeer {
                peer: p.clone(),
                down_seconds: 3608,
            })
            .collect(),
        peers.iter().take(pending).cloned().collect(),
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_keys = keys;
    r
}
#[test]
fn state_task_15_region_classify_go_sequence() {
    let c = config();
    let mut s = RegionStatistics::new();
    let mut stores: Vec<_> = (1..=4)
        .map(|id| {
            StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            })
        })
        .collect();
    stores[3].set_store_state(StoreState::Offline, Some(false));
    let r = region(1, 4, true, 0, 0, 0, 0, false);
    assert!(s.region_stats_need_update(&r, &c));
    s.observe(&r, &stores, &c, 100).unwrap();
    assert_eq!(s.bitmap(1), Some(2 | 16 | 32 | 64 | 256));
    let keyed = region(1, 4, true, 0, 0, 1, 2, false);
    assert!(s.region_stats_need_update(&keyed, &c));
    s.observe(&keyed, &stores, &c, 101).unwrap();
    assert!(!s.is_region_stats_type(1, EmptyRegion));
    let large = region(1, 4, true, 2, 1, 144, 0, false);
    s.observe(&large, &stores, &c, 102).unwrap();
    assert_eq!(s.bitmap(1), Some(2 | 4 | 8 | 16 | 32 | 128));
    let r2 = region(2, 2, false, 1, 0, 0, 0, false);
    s.observe(&r2, &stores[..2], &c, 103).unwrap();
    assert_eq!(s.region_ids_by_type(DownPeer), vec![1, 2]);
    assert_eq!(s.region_ids_by_type(MissPeer), vec![2]);
    let removed = region(1, 3, false, 2, 1, 144, 0, false);
    s.observe(&removed, &stores, &c, 104).unwrap();
    assert_eq!(s.bitmap(1), Some(4 | 8 | 128));
    stores[3].set_store_state(StoreState::Up, None);
    s.observe(&removed, &stores, &c, 105).unwrap();
    assert!(s.region_ids_by_type(OfflinePeer).is_empty());
    assert!(!s.region_stats_need_update(&removed, &c));
    s.clear_defunct_region(1);
    assert!(!s.is_region_stats_type(1, DownPeer));
    assert!(!s.is_region_stats_type(1, PendingPeer));
    assert!(!s.is_region_stats_type(1, OversizedRegion));
    assert_eq!(s.region_ids_by_type(DownPeer), vec![2]);
}
#[test]
fn state_task_15_region_classify_clear_query_witness() {
    let c = config();
    let mut s = RegionStatistics::new();
    let r = region(5, 3, false, 0, 0, 50, 1, true);
    s.observe(&r, &[], &c, 100).unwrap();
    assert_eq!(s.bitmap(5), Some(512));
    let mut latest = r.clone();
    latest.stats_mut().approximate_keys = 99;
    let provider = |id| if id == 5 { Some(latest.clone()) } else { None };
    let mut results = s.get_region_stats_by_type(WitnessLeader, &provider);
    assert_eq!(results[0].stats().approximate_keys, 99);
    results[0].stats_mut().approximate_keys = 1;
    assert_eq!(latest.stats().approximate_keys, 99);
    assert!(
        s.get_region_stats_by_type(WitnessLeader, &|_| None)
            .is_empty()
    );
    s.clear_defunct_region(5);
    s.clear_defunct_region(5);
    assert!(!s.is_observed(5));
    assert!(s.region_ids_by_type(WitnessLeader).is_empty());
    let r = region(5, 3, false, 0, 0, 50, 1, false);
    s.observe(&r, &[], &c, 101).unwrap();
    assert_eq!(s.bitmap(5), Some(0));
    assert!(s.region_ids_by_type(WitnessLeader).is_empty());
    let mut no_leader = RegionInfo::new(r.meta().clone(), None, vec![], vec![]);
    *no_leader.stats_mut() = r.stats().clone();
    s.observe(&no_leader, &[], &c, 102).unwrap();
    assert!(!s.is_region_stats_type(5, WitnessLeader));
    s.clear_defunct_region(5);
    assert!(s.region_stats_need_update(&r, &c));
}
#[test]
fn state_task_15_region_classify_thresholds_and_config() {
    let mut c = config();
    let mut s = RegionStatistics::new();
    for (size, keys, over, small, empty) in [
        (143, 1, false, false, false),
        (144, 1, true, false, false),
        (1, 1440000, true, false, false),
        (20, 200000, false, true, false),
        (20, 200001, false, false, false),
        (1, 0, false, true, true),
        (1, 1, false, true, false),
        (2, 0, false, true, false),
    ] {
        let r = region(1, 3, false, 0, 0, size, keys, false);
        s.observe(&r, &[], &c, 100).unwrap();
        assert_eq!(s.is_region_stats_type(1, OversizedRegion), over);
        assert_eq!(s.is_region_stats_type(1, UndersizedRegion), small);
        assert_eq!(s.is_region_stats_type(1, EmptyRegion), empty);
        assert!(!s.region_stats_need_update(&r, &c));
    }
    let r = region(1, 3, false, 0, 0, 100, 10, false);
    s.observe(&r, &[], &c, 101).unwrap();
    c.store.coprocessor.region_max_size = "80MiB".into();
    c.store.region_max_size_mb = 80;
    assert!(s.region_stats_need_update(&r, &c));
    s.observe(&r, &[], &c, 102).unwrap();
    assert!(s.is_region_stats_type(1, OversizedRegion));
    c.store.coprocessor.region_max_size = "200MiB".into();
    c.store.region_max_size_mb = 200;
    c.schedule.max_merge_region_size = 100;
    assert!(s.region_stats_need_update(&r, &c));
    s.observe(&r, &[], &c, 103).unwrap();
    assert!(!s.is_region_stats_type(1, OversizedRegion));
    assert!(s.is_region_stats_type(1, UndersizedRegion));
    c.store.coprocessor.region_max_keys = 10;
    assert!(s.region_stats_need_update(&r, &c));
    s.observe(&r, &[], &c, 104).unwrap();
    assert!(s.is_region_stats_type(1, OversizedRegion));
    c.schedule.max_merge_region_keys = 9;
    assert!(s.region_stats_need_update(&r, &c));
    s.observe(&r, &[], &c, 105).unwrap();
    assert!(!s.is_region_stats_type(1, UndersizedRegion));
    c.replication.max_replicas = 4;
    assert!(!s.region_stats_need_update(&r, &c)); // Exact Go trigger subset.
    s.observe(&r, &[], &c, 104).unwrap();
    assert!(s.is_region_stats_type(1, MissPeer));
}
#[test]
fn state_task_15_region_classify_durations_and_reset() {
    let c = config();
    let mut s = RegionStatistics::new();
    let r = region(1, 2, false, 1, 0, 50, 1, false);
    assert_eq!(s.observe(&r, &[], &c, 100).unwrap(), Default::default());
    let d = s.observe(&r, &[], &c, 107).unwrap();
    assert_eq!(
        (d.miss_voter_seconds, d.down_peer_seconds),
        (Some(7), Some(7))
    );
    let d = s.observe(&r, &[], &c, 110).unwrap();
    assert_eq!(
        (d.miss_voter_seconds, d.down_peer_seconds),
        (Some(10), Some(10))
    );
    s.observe(&region(1, 3, false, 0, 0, 50, 1, false), &[], &c, 111)
        .unwrap();
    assert_eq!(s.observe(&r, &[], &c, 120).unwrap(), Default::default());
    assert_eq!(
        s.observe(&r, &[], &c, 122).unwrap().down_peer_seconds,
        Some(2)
    );
    s.clear_defunct_region(1);
    assert_eq!(s.observe(&r, &[], &c, 130).unwrap(), Default::default());
    // Match Go's zero timestamp sentinel rather than treating epoch as initialized.
    s.clear_defunct_region(1);
    s.observe(&r, &[], &c, 0).unwrap();
    assert_eq!(s.observe(&r, &[], &c, 2).unwrap(), Default::default());
    assert_eq!(
        s.observe(&r, &[], &c, 1).unwrap().down_peer_seconds,
        Some(-1)
    );
}
#[test]
fn state_task_15_region_classify_special_transitions_and_rule_boundary() {
    let mut c = config();
    let mut s = RegionStatistics::new();
    let base = region(1, 3, false, 0, 0, 50, 1, false);
    s.observe(&base, &[], &c, 1).unwrap();
    for r in [
        region(1, 3, false, 1, 0, 50, 1, false),
        region(1, 3, false, 0, 1, 50, 1, false),
        region(1, 3, true, 0, 0, 50, 1, false),
        region(1, 3, false, 0, 0, 0, 0, false),
    ] {
        assert!(s.region_stats_need_update(&r, &c));
        s.observe(&r, &[], &c, 2).unwrap();
        assert!(s.region_stats_need_update(&base, &c));
        s.observe(&base, &[], &c, 3).unwrap();
    }
    c.replication.enable_placement_rules = true;
    assert!(
        s.observe(&region(2, 1, false, 0, 0, 0, 0, false), &[], &c, 4)
            .is_err()
    );
    assert!(!s.is_observed(2));
    assert_eq!(s.bitmap(1), Some(0));
}
