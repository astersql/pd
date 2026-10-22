// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::{Arc, Barrier, Mutex};
fn region(id: u64, version: u64, conf_ver: u64, term: u64, pending: bool) -> Arc<RegionInfo> {
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            epoch: Some(RegionEpoch { version, conf_ver }),
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer.clone()),
        vec![],
        if pending { vec![peer] } else { vec![] },
    );
    r.term = term;
    Arc::new(r)
}
#[test]
fn core_task_48_eventual_repair_with_barrier() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_48.json")).unwrap();
    let old = region(1, 100, 100, 5, true);
    let new = region(1, 100, 100, 5, false);
    let registry = Arc::new(Mutex::new(RegionsInfo::new()));
    registry
        .lock()
        .unwrap()
        .check_and_put_root_tree(old.clone())
        .unwrap();
    let ready = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let worker = {
        let (registry, ready, resume, old) =
            (registry.clone(), ready.clone(), resume.clone(), old.clone());
        std::thread::spawn(move || {
            ready.wait();
            resume.wait();
            registry
                .lock()
                .unwrap()
                .update_sub_tree_order_insensitive(old);
        })
    };
    ready.wait();
    {
        let mut r = registry.lock().unwrap();
        r.check_and_put_root_tree(new.clone()).unwrap();
        r.check_and_put_sub_tree(&new);
    }
    resume.wait();
    worker.join().unwrap();
    let mut r = registry.lock().unwrap();
    assert_eq!(new.get_ref(), f["after_replay"][0].as_i64().unwrap() as i32);
    assert_eq!(old.get_ref(), f["after_replay"][1].as_i64().unwrap() as i32);
    assert!(
        r.role_tree(RegionRole::Pending, 1)
            .unwrap()
            .search(b"a")
            .is_some()
    );
    r.check_and_put_sub_tree(&old);
    r.check_and_put_sub_tree(&old);
    assert_eq!(new.get_ref(), 2);
    assert_eq!(old.get_ref(), 0);
    assert!(
        r.role_tree(RegionRole::Pending, 1)
            .unwrap()
            .search(b"a")
            .is_none()
    );
}
#[test]
fn core_task_48_stale_and_recreated() {
    let current = region(1, 100, 100, 5, false);
    let mut r = RegionsInfo::new();
    r.update_sub_tree_order_insensitive(current.clone());
    for old in [
        region(1, 99, 100, 5, true),
        region(1, 100, 99, 5, true),
        region(1, 100, 100, 4, true),
    ] {
        r.update_sub_tree_order_insensitive(old.clone());
        assert_eq!(old.get_ref(), 0);
        assert_eq!(current.get_ref(), 1);
    }
    let unknown = region(1, 100, 100, 0, true);
    r.update_sub_tree_order_insensitive(unknown.clone());
    assert_eq!(unknown.get_ref(), 1);
    let recreated = region(1, 1, 1, 1, false);
    r.update_sub_tree_order_insensitive(recreated.clone());
    assert_eq!(recreated.get_ref(), 1);
    assert_eq!(unknown.get_ref(), 0);
}
#[test]
fn core_task_48_missing_root_and_overlap_recheck() {
    let mut r = RegionsInfo::new();
    let old = region(1, 100, 100, 5, true);
    r.check_and_put_sub_tree(&old);
    assert_eq!(old.get_ref(), 0);
    assert!(r.role_tree(RegionRole::Pending, 1).is_none());
    r.update_sub_tree_order_insensitive(old.clone());
    let new = region(2, 101, 100, 5, false);
    r.check_and_put_root_tree(new.clone()).unwrap();
    r.check_and_put_sub_tree(&new);
    assert_eq!(old.get_ref(), 0);
    assert_eq!(new.get_ref(), 2);
    r.check_and_put_sub_tree(&old);
    assert_eq!(old.get_ref(), 0);
    assert!(
        r.role_tree(RegionRole::Pending, 1)
            .unwrap()
            .search(b"a")
            .is_none()
    );
}
