// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::*;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_statistics::{
    HotPeerCache, LabelStatistics, RegionStatisticType, RegionStatistics,
};
use astersql_pd_statistics_utils::RWType;
use std::collections::BTreeSet;

fn report(id: u64, start: &str, end: &str, version: u64, stores: &[u64]) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&store_id| Peer {
            id: id * 10 + store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 2,
            }),
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 10;
    r.set_source(RegionSource::Heartbeat);
    r
}
fn observe(s: &mut RegionStatistics, l: &mut LabelStatistics, r: &RegionInfo) {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    store.set_store_state(StoreState::Offline, Some(false));
    s.observe(r, &[store], &conf, 10).unwrap();
    l.observe(r, &[], &[]);
}
fn apply(
    c: &mut BasicCluster,
    s: &mut RegionStatistics,
    l: &mut LabelStatistics,
    r: RegionInfo,
) -> RegionHeartbeatResult {
    let result = handle_region_heartbeat_with_overlap_cleanup(c, r, false, Some(s), l).unwrap();
    observe(s, l, &result.region);
    result
}
#[test]
fn state_task_44_region_overlap_split_left_then_right_and_empty_end() {
    let (mut c, mut s, mut l) = (
        BasicCluster::new(),
        RegionStatistics::new(),
        LabelStatistics::new(),
    );
    apply(&mut c, &mut s, &mut l, report(1, "", "", 2, &[1, 2]));
    let left = apply(&mut c, &mut s, &mut l, report(2, "", "m", 2, &[1, 3]));
    assert_eq!(
        left.overlaps
            .iter()
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert!(c.regions.get_region(1).is_none());
    assert_eq!(
        c.regions.get_range_holes(),
        vec![["6D".to_owned(), String::new()]]
    );
    assert_eq!(c.regions.store_stats(2).region, 0);
    assert!(
        !s.is_observed(1),
        "evicted whole-range anomaly must be cleared"
    );
    assert_eq!(l.label_counter()["none"], 2); // deferred
    l.clear_defunct_regions();
    assert_eq!(l.label_counter()["none"], 1);
    apply(&mut c, &mut s, &mut l, report(1, "m", "", 3, &[1, 2]));
    assert!(c.regions.get_range_holes().is_empty());
    assert_eq!(c.regions.store_stats(1).region, 2);
    assert_eq!(c.regions.store_stats(3).region, 1);
    apply(&mut c, &mut s, &mut l, report(1, "q", "", 4, &[1, 2]));
    assert_eq!(
        c.regions.get_range_holes(),
        vec![["6D".to_owned(), "71".to_owned()]]
    );
    assert!(s.is_observed(1)); // same-ID shrink is not an overlap eviction
    apply(&mut c, &mut s, &mut l, report(3, "m", "q", 2, &[1, 4]));
    assert_eq!(
        s.region_ids_by_type(RegionStatisticType::OfflinePeer),
        vec![1, 2, 3]
    );
    assert!(c.regions.get_range_holes().is_empty());
    assert_eq!(c.regions.store_stats(1).region, 3);
}
#[test]
fn state_task_44_region_overlap_merge_offline_cross_store_and_replay() {
    let (mut c, mut s, mut l) = (
        BasicCluster::new(),
        RegionStatistics::new(),
        LabelStatistics::new(),
    );
    apply(&mut c, &mut s, &mut l, report(1, "", "m", 2, &[1, 2]));
    apply(&mut c, &mut s, &mut l, report(2, "m", "", 2, &[1, 3]));
    let merged = apply(&mut c, &mut s, &mut l, report(2, "", "", 3, &[1, 4]));
    assert_eq!(merged.overlaps.len(), 1);
    assert_eq!(
        s.region_ids_by_type(RegionStatisticType::OfflinePeer),
        vec![2]
    );
    assert_eq!(c.regions.store_stats(1).region, 1);
    for store in [2, 3] {
        assert_eq!(c.regions.store_stats(store).region, 0);
    }
    assert_eq!(c.regions.store_stats(4).region, 1);
    assert_eq!(c.regions.store_stats(4).region_size, 10);
    l.clear_defunct_regions();
    assert_eq!(l.label_counter()["none"], 1);
    let labels = l.label_counter();
    assert!(
        handle_region_heartbeat_with_overlap_cleanup(
            &mut c,
            report(1, "", "m", 2, &[1, 2]),
            false,
            Some(&mut s),
            &mut l
        )
        .is_err()
    );
    assert!(!s.is_observed(1));
    assert_eq!(l.label_counter(), labels);
    assert!(c.regions.get_region(1).is_none());
}
#[test]
fn state_task_44_region_overlap_optional_statistics_and_duplicate_label_cleanup() {
    let (mut c, mut l) = (BasicCluster::new(), LabelStatistics::new());
    let r = report(1, "", "", 2, &[1]);
    handle_region_heartbeat(&mut c, r.clone(), false).unwrap();
    l.observe(&r, &[], &[]);
    let new = report(2, "", "", 3, &[2]);
    handle_region_heartbeat_with_overlap_cleanup(&mut c, new.clone(), false, None, &mut l).unwrap();
    l.observe(&new, &[], &[]);
    handle_region_heartbeat_with_overlap_cleanup(&mut c, new, false, None, &mut l).unwrap();
    l.clear_defunct_regions();
    l.clear_defunct_regions();
    assert_eq!(l.label_counter()["none"], 1);
}
#[test]
fn state_task_44_region_overlap_hot_history_uses_normal_ttl() {
    for kind in [RWType::READ, RWType::WRITE] {
        let (mut c, mut s, mut l) = (
            BasicCluster::new(),
            RegionStatistics::new(),
            LabelStatistics::new(),
        );
        let old = report(1, "", "", 2, &[1, 2]);
        apply(&mut c, &mut s, &mut l, old.clone());
        let mut hot = HotPeerCache::new(kind);
        hot.set_peer_ttl(100);
        let mut loads = [0.; 8];
        for dim in kind.region_stats() {
            loads[dim.as_i32() as usize] = 6_000_000.;
        }
        let actions = hot.check_peer_flow(&old, None, &loads, kind.report_interval(), |_| [1.; 4]);
        for item in actions {
            hot.update_stat_at(item, 0);
        }
        assert!(!hot.stores_of_region(1).is_empty());
        apply(&mut c, &mut s, &mut l, report(2, "", "", 3, &[3]));
        assert!(!s.is_observed(1));
        assert!(!hot.stores_of_region(1).is_empty());
        let stores = BTreeSet::from([1, 2, 3]);
        hot.gc(99, &stores);
        assert!(!hot.stores_of_region(1).is_empty());
        hot.gc(100, &stores);
        assert!(hot.stores_of_region(1).is_empty());
    }
}

#[test]
fn state_task_44_region_overlap_go_repeated_split_merge_sequence() {
    let (mut c, mut s, mut l) = (
        BasicCluster::new(),
        RegionStatistics::new(),
        LabelStatistics::new(),
    );
    let mut regions = vec![report(1, "", "", 2, &[1, 2, 3])];
    for phase in 0..28 {
        regions = if phase < 7 || (phase >= 14 && (phase - 14) % 3 != 2) {
            split_regions(&regions)
        } else {
            merge_regions(&mut regions)
        };
        for r in &regions {
            // Synthetic MergeRegions returns an unclassified literal in Go/Rust.
            // Rebuild membership like a real heartbeat constructor before testing
            // store-role counts; keep the helper's IDs, ranges and epochs.
            apply(&mut c, &mut s, &mut l, r.clone_with([]));
        }
        l.clear_defunct_regions();
        assert!(c.regions.get_range_holes().is_empty(), "phase {phase}");
        assert_eq!(c.regions.tree_len(), regions.len(), "phase {phase}");
        assert_eq!(
            s.region_ids_by_type(RegionStatisticType::OfflinePeer).len(),
            regions.len(),
            "phase {phase}"
        );
        assert_eq!(
            l.label_counter()["none"],
            regions.len() as i64,
            "phase {phase}"
        );
        for store in [1, 2, 3] {
            assert_eq!(
                c.regions.store_stats(store).region,
                regions.len(),
                "phase {phase}"
            );
        }
    }
}
