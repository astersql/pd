// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;

fn put(regions: &mut RegionsInfo, id: u64, active: bool, size: u64) {
    let peers = vec![
        Peer {
            id: 1,
            store_id: 1,
            role: PeerRole::Voter,
            is_witness: false,
        },
        Peer {
            id: 2,
            store_id: 2,
            role: PeerRole::Voter,
            is_witness: true,
        },
        Peer {
            id: 3,
            store_id: 3,
            role: PeerRole::Learner,
            is_witness: false,
        },
    ];
    let mut region = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: vec![id as u8],
                end_key: vec![id as u8 + 1],
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![peers[2].clone()],
    );
    if active {
        region.set_heartbeat_stats(
            RegionHeartbeatStats {
                approximate_size: size * 1048576,
                written_bytes: 100,
                written_keys: 20,
                ..Default::default()
            },
            0,
        );
        region.set_report_interval(10, 20);
    }
    let region = Arc::new(region);
    let (old, overlaps, changed) = regions.set_region(region.clone());
    regions.update_sub_tree(region, old.as_deref(), &overlaps, changed);
}

#[test]
fn core_task_53_roles_updates_and_removal() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_53.json")).unwrap();
    let mut r = RegionsInfo::new();
    put(&mut r, 1, true, 5);
    put(&mut r, 2, true, 8);
    for (store, role) in [
        (1, RegionRole::Leader),
        (2, RegionRole::Follower),
        (3, RegionRole::Learner),
    ] {
        assert_eq!(r.store_role_count(store, role), 2);
        assert_eq!(
            r.store_role_size(store, role),
            fixture["size"].as_i64().unwrap()
        );
        assert_eq!(r.store_role_write_rate(store, role), (20., 4.));
        assert_eq!(r.store_region_count(store), 2);
        assert_eq!(r.store_region_size(store), 13);
        assert_eq!(r.store_write_rate(store), (20., 4.));
        assert_eq!(r.not_from_storage_regions_count_by_store(store), 2);
    }
    assert_eq!(r.store_role_count(2, RegionRole::Witness), 2);
    assert_eq!(r.store_role_count(3, RegionRole::Pending), 2);
    assert_eq!(r.total_region_count(), 2);
    assert_eq!(
        r.store_stats(3),
        StoreRegionStats {
            leader: 0,
            region: 2,
            witness: 0,
            learner: 2,
            pending: 2,
            leader_size: 0,
            region_size: 13,
        }
    );
    assert_eq!(r.store_stats(99), StoreRegionStats::default());
    assert_eq!(r.average_region_size(), 6);
    assert_eq!(r.not_from_storage_regions_count(), 2);
    put(&mut r, 1, true, 10);
    assert_eq!(r.store_region_size(2), 18);
    r.remove_region_if_exist(2);
    assert_eq!(r.average_region_size(), 10);
    assert_eq!(r.store_region_count(3), 1);
    assert_eq!(r.store_write_rate(3), (10., 2.));
    r.remove_region_if_exist(1);
    assert_eq!(r.store_region_size(2), 0);
    assert_eq!(r.store_write_rate(2), (0., 0.));
    assert_eq!(r.not_from_storage_regions_count(), 0);
}

#[test]
fn core_task_53_prepared_threshold_and_empty() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_53.json")).unwrap();
    let mut r = RegionsInfo::new();
    assert_eq!(r.average_region_size(), 0);
    assert_eq!(r.store_region_count(99), 0);
    assert_eq!(r.store_role_write_rate(99, RegionRole::Leader), (0., 0.));
    assert!(r.is_store_prepared(99));
    for id in 1..=10 {
        put(&mut r, id, id <= 8, 1);
    }
    assert!(!r.is_store_prepared(1));
    put(&mut r, 9, true, 1);
    assert_eq!(
        r.not_from_storage_regions_count_by_store(1),
        fixture["threshold_active"].as_i64().unwrap()
    );
    assert!(r.is_store_prepared(1));
    put(&mut r, 9, false, 0);
    assert!(!r.is_store_prepared(1));
    assert_eq!(r.not_from_storage_regions_count(), 8);
    assert_eq!(r.store_role_count(2, RegionRole::Witness), 10);
    assert_eq!(r.store_region_count(2), 10);
}
