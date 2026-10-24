// Copyright 2026 AsterSQL.
use crate::{
    HotPeerSnapshot, RegionStatInformer, RegionStats, get_region_stats, with_store_map_option,
};
use astersql_pd_core::{Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::RWType;
use std::collections::BTreeMap;

fn region(
    id: u64,
    stores: &[u64],
    leader: Option<u64>,
    size: i64,
    kv: i64,
    keys: i64,
) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&store_id| Peer {
            id: store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let leader = leader.map(|store| Peer {
        id: store,
        store_id: store,
        ..Default::default()
    });
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            peers,
            ..Default::default()
        },
        leader,
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_kv_size = kv;
    r.stats_mut().approximate_keys = keys;
    r
}
fn api_regions() -> Vec<RegionInfo> {
    vec![
        region(1, &[1, 2, 3], Some(1), 100, 80, 50),
        region(2, &[1, 4, 5], Some(4), 200, 180, 150),
        region(3, &[1, 5], Some(5), 1, 1, 0),
        region(4, &[4], Some(4), 50, 30, 20),
    ]
}
// Host snapshot input boundary, not a simulated aggregation or hotness algorithm.
#[derive(Default)]
struct Snapshots(BTreeMap<(i32, u64, u64), HotPeerSnapshot>);
impl RegionStatInformer for Snapshots {
    fn get_hot_peer_stat(
        &self,
        rw: RWType,
        region_id: u64,
        store_id: u64,
    ) -> Option<HotPeerSnapshot> {
        self.0.get(&(rw.as_i32(), region_id, store_id)).copied()
    }
}
#[test]
fn state_task_14_region_totals_ia_cap_and_nil_leader() {
    let mut r = region(1, &[], None, 0, 100, 0);
    r.stats_mut().approximate_ia_kv_size = 40;
    assert_eq!(
        get_region_stats(&[r.clone()], None, &[]).user_ia_storage_size,
        40
    );
    r.stats_mut().approximate_ia_kv_size = 120;
    r.stats_mut().approximate_columnar_kv_size = 7;
    let s = get_region_stats(&[r], None, &[]);
    assert_eq!(
        (
            s.count,
            s.user_storage_size,
            s.user_ia_storage_size,
            s.user_columnar_storage_size
        ),
        (1, 100, 100, 7)
    );
    assert!(s.store_leader_count.is_empty() && s.store_peer_count.is_empty());
}
#[test]
fn state_task_14_region_totals_empty_uses_keys() {
    let regions = [
        region(1, &[], None, 1, 0, 0),
        region(2, &[], None, 1, 0, 1),
        region(3, &[], None, 2, 0, 0),
    ];
    let s = get_region_stats(&regions, None, &[]);
    assert_eq!(
        (s.count, s.empty_count, s.storage_size, s.storage_keys),
        (3, 1, 4, 1)
    );
    assert_eq!(get_region_stats(&[], None, &[]), RegionStats::default());
}
#[test]
fn state_task_14_region_totals_go_api_distribution() {
    let regions = api_regions();
    let s = get_region_stats(&regions, None, &[]);
    assert_eq!(
        (
            s.count,
            s.empty_count,
            s.storage_size,
            s.user_storage_size,
            s.storage_keys
        ),
        (4, 1, 351, 291, 220)
    );
    assert_eq!(
        s.store_leader_count,
        BTreeMap::from([(1, 1), (4, 2), (5, 1)])
    );
    assert_eq!(
        s.store_peer_count,
        BTreeMap::from([(1, 3), (2, 1), (3, 1), (4, 2), (5, 2)])
    );
    assert_eq!(
        s.store_leader_size,
        BTreeMap::from([(1, 100), (4, 250), (5, 1)])
    );
    assert_eq!(
        s.store_leader_keys,
        BTreeMap::from([(1, 50), (4, 170), (5, 0)])
    );
    assert_eq!(
        s.store_peer_size,
        BTreeMap::from([(1, 301), (2, 100), (3, 100), (4, 250), (5, 201)])
    );
    assert_eq!(
        s.store_peer_keys,
        BTreeMap::from([(1, 200), (2, 50), (3, 50), (4, 170), (5, 150)])
    );
    let sub = get_region_stats(&regions[1..3], None, &[]);
    assert_eq!(
        (
            sub.count,
            sub.empty_count,
            sub.storage_size,
            sub.user_storage_size,
            sub.storage_keys
        ),
        (2, 1, 201, 181, 150)
    );
    assert_eq!(
        sub.store_peer_size,
        BTreeMap::from([(1, 201), (4, 200), (5, 201)])
    );
}
#[test]
fn state_task_14_region_totals_go_api_hot_filter() {
    let regions = api_regions();
    let inputs = Snapshots(BTreeMap::from([
        (
            (RWType::READ.as_i32(), 1, 1),
            HotPeerSnapshot {
                loads: [10000.0, 1000.0, 100.0, 999.0],
            },
        ),
        (
            (RWType::WRITE.as_i32(), 1, 1),
            HotPeerSnapshot {
                loads: [50000.0, 5000.0, 500.0, 999.0],
            },
        ),
        (
            (RWType::READ.as_i32(), 1, 2),
            HotPeerSnapshot {
                loads: [7.0, 8.0, 9.0, 0.0],
            },
        ),
    ]));
    let stores = BTreeMap::from([(1, "tikv".to_owned())]);
    let filter = with_store_map_option(&stores);
    let s = get_region_stats(&regions, Some(&inputs), &[&filter]);
    assert_eq!(
        (s.count, s.storage_size, s.user_storage_size, s.storage_keys),
        (4, 351, 291, 220)
    );
    assert_eq!(s.store_peer_count, BTreeMap::from([(1, 3)]));
    assert_eq!(s.store_peer_size, BTreeMap::from([(1, 301)]));
    assert_eq!(s.store_leader_size, BTreeMap::from([(1, 100)]));
    assert_eq!(s.store_write_bytes, BTreeMap::from([(1, 50000)]));
    assert_eq!(s.store_write_keys, BTreeMap::from([(1, 5000)]));
    assert_eq!(s.store_write_query, BTreeMap::from([(1, 500)]));
    assert_eq!(s.store_leader_read_bytes, BTreeMap::from([(1, 10000)]));
    assert_eq!(s.store_leader_read_keys, BTreeMap::from([(1, 1000)]));
    assert_eq!(s.store_leader_read_query, BTreeMap::from([(1, 100)]));
    assert_eq!(s.store_peer_read_bytes, s.store_leader_read_bytes);
    assert_eq!(s.store_peer_read_keys, s.store_leader_read_keys);
    assert_eq!(s.store_peer_read_query, s.store_leader_read_query);
    assert!(s.store_engine.is_empty());
    let all = get_region_stats(&regions, Some(&inputs), &[]);
    assert_eq!(all.store_peer_read_bytes.get(&2), Some(&7));
    assert!(!all.store_leader_read_bytes.contains_key(&2));
    let reject = |_: &Peer| false;
    let none = get_region_stats(&regions, Some(&inputs), &[&filter, &reject]);
    assert_eq!(none.storage_size, 351);
    assert!(
        none.store_peer_count.is_empty()
            && none.store_leader_count.is_empty()
            && none.store_write_bytes.is_empty()
    );
}
#[test]
fn state_task_14_region_totals_witness_tiflash_and_no_leader() {
    let peers = vec![
        Peer {
            id: 1,
            store_id: 1,
            is_witness: true,
            ..Default::default()
        },
        Peer {
            id: 2,
            store_id: 2,
            role: PeerRole::Learner,
            ..Default::default()
        },
    ];
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 80;
    r.stats_mut().approximate_keys = 90;
    r.stats_mut().approximate_columnar_kv_size = 120;
    let mut stats = RegionStats::default();
    stats.store_engine.insert(2, "tiflash".into());
    stats.observe(&r, None, &[]);
    assert_eq!(stats.store_peer_count, BTreeMap::from([(1, 1), (2, 1)]));
    assert_eq!(stats.store_peer_size, BTreeMap::from([(1, 0), (2, 80)]));
    assert_eq!(stats.store_peer_keys, BTreeMap::from([(1, 0), (2, 90)]));
    assert!(stats.store_leader_count.is_empty());
    assert_eq!(stats.store_engine.get(&2).unwrap(), "tiflash");
    assert_eq!(r.stats().approximate_size, 80);
}
#[test]
fn state_task_14_region_totals_accumulation_and_wrapping() {
    let mut r = region(1, &[0], Some(0), i64::MAX, i64::MAX, i64::MAX);
    r.stats_mut().approximate_ia_kv_size = i64::MAX;
    let inputs = Snapshots(BTreeMap::from([(
        (RWType::READ.as_i32(), 1, 0),
        HotPeerSnapshot {
            loads: [(1u64 << 63) as f64, 0.0, 0.0, 0.0],
        },
    )]));
    let mut s = get_region_stats(&[r.clone()], Some(&inputs), &[]);
    s.observe(&r, Some(&inputs), &[]);
    assert_eq!(s.count, 2);
    assert_eq!(s.storage_size, -2);
    assert_eq!(s.user_ia_storage_size, -2);
    assert_eq!(s.store_peer_size[&0], -2);
    assert_eq!(s.store_leader_read_bytes[&0], 0);
    assert_eq!(s.store_peer_read_keys.get(&0), Some(&0));
}
