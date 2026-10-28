// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(id: u64) -> Arc<RegionInfo> {
    let peers = vec![
        Peer {
            id: 1,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 2,
            store_id: 2,
            is_witness: true,
            ..Default::default()
        },
        Peer {
            id: 3,
            store_id: 3,
            role: PeerRole::Learner,
            ..Default::default()
        },
    ];
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![peers[2].clone()],
    ))
}
fn put(r: &mut RegionsInfo, v: Arc<RegionInfo>) {
    let (old, overlaps, changed) = r.set_region(v.clone());
    r.update_sub_tree(v, old.as_deref(), &overlaps, changed);
}
fn roles() -> [(RegionRole, u64); 5] {
    [
        (RegionRole::Leader, 1),
        (RegionRole::Follower, 2),
        (RegionRole::Witness, 2),
        (RegionRole::Learner, 3),
        (RegionRole::Pending, 3),
    ]
}
#[test]
fn core_task_49_remove_split_and_missing() {
    let mut r = RegionsInfo::new();
    let v = region(1);
    put(&mut r, v.clone());
    r.remove_region_if_exist(99);
    assert_eq!(v.get_ref(), 2);
    r.remove_region(&v);
    assert_eq!(v.get_ref(), 1);
    assert_eq!(r.tree_len(), 0);
    assert!(r.get_region(1).is_none());
    assert!(
        r.role_tree(RegionRole::Leader, 1)
            .unwrap()
            .search(b"")
            .is_some()
    );
    r.remove_region_from_sub_tree(&v);
    r.remove_region_from_sub_tree(&v);
    assert_eq!(v.get_ref(), 0);
    for (role, store) in roles() {
        let t = r.role_tree(role, store).unwrap();
        assert!(t.search(b"").is_none());
        assert_eq!(t.total_size(), 0);
    }
    put(&mut r, v.clone());
    r.remove_region_if_exist(1);
    r.remove_region_if_exist(1);
    assert_eq!(v.get_ref(), 0);
    assert!(r.get_regions().is_empty());
    assert!(r.read().subtrees.regions.is_empty());
}
#[test]
fn core_task_49_reset_and_reinsert() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_49.json")).unwrap();
    let mut r = RegionsInfo::new();
    let old = region(1);
    put(&mut r, old.clone());
    r.reset_region_cache();
    assert_eq!(
        old.get_ref(),
        f["old_ref_after_reset"].as_i64().unwrap() as i32
    );
    assert_eq!(r.tree_len(), 0);
    assert!(r.get_regions().is_empty());
    assert!(r.read().subtrees.regions.is_empty());
    for (role, store) in roles() {
        assert!(r.role_tree(role, store).is_none());
    }
    r.reset_region_cache();
    let new = region(1);
    assert_eq!(new.get_ref(), 0);
    put(&mut r, new.clone());
    assert_eq!(new.get_ref(), 2);
}
#[test]
fn core_task_49_flow_replacement_and_reset() {
    let mut r = RegionsInfo::new();
    let old = region(1);
    put(&mut r, old.clone());
    let mut next = (*old).clone();
    next.stats.written_bytes = 100;
    let next = Arc::new(next);
    put(&mut r, next.clone());
    put(&mut r, next.clone());
    assert_eq!(old.get_ref(), 0);
    assert_eq!(next.get_ref(), 2);
    r.remove_region_if_exist(1);
    assert_eq!(next.get_ref(), 0);
    r.reset_region_cache();
    assert_eq!(next.get_ref(), 0);
}
