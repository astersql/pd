// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::*;
use std::sync::Arc;

fn report(id: u64, start: &str, end: &str, version: u64, conf_ver: u64) -> RegionInfo {
    let peer = Peer {
        id: id + 10,
        store_id: 1,
        ..Default::default()
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            epoch: Some(RegionEpoch { version, conf_ver }),
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    r.set_source(RegionSource::Heartbeat);
    r
}
fn cached(c: &BasicCluster, id: u64) -> Arc<RegionInfo> {
    c.regions.get_region(id).unwrap()
}
fn check_indexes(c: &BasicCluster, id: u64, size: i64) {
    let root = cached(c, id);
    let sub = c
        .regions
        .role_tree(RegionRole::Leader, 1)
        .unwrap()
        .search(&root.meta().key_range.start_key)
        .unwrap();
    assert!(Arc::ptr_eq(&root, &sub));
    assert_eq!(sub.stats().approximate_size, size);
    assert_eq!(c.regions.store_stats(1).region_size, size);
}
#[test]
fn state_task_43_region_heartbeat_accept_duplicate_inherit() {
    let mut c = BasicCluster::new();
    let mut r = report(1, "", "", 2, 2);
    r.stats_mut().approximate_size = 10;
    r.stats_mut().approximate_keys = 20;
    r.update_buckets(
        Some(&RegionBuckets {
            region_id: 1,
            version: 7,
            keys: vec![],
        }),
        None,
    );
    let first = handle_region_heartbeat(&mut c, r.clone(), true).unwrap();
    assert!(first.guide.save_cache && first.guide.save_kv && first.guide.retained);
    check_indexes(&c, 1, 10);
    let original = cached(&c, 1);
    let duplicate = handle_region_heartbeat(&mut c, r, true).unwrap();
    assert_eq!(duplicate.guide, RegionGuide::default());
    assert!(Arc::ptr_eq(&original, &cached(&c, 1)));
    let mut missing = report(1, "", "", 3, 2);
    missing.stats_mut().approximate_keys = 999;
    let inherited = handle_region_heartbeat(&mut c, missing, true).unwrap();
    assert_eq!(inherited.region.stats().approximate_keys, 20);
    assert_eq!(inherited.region.buckets().unwrap().version, 7);
    check_indexes(&c, 1, 10);
    let empty = handle_region_heartbeat(&mut BasicCluster::new(), report(2, "x", "z", 2, 2), false)
        .unwrap();
    assert_eq!(empty.region.stats().approximate_size, 1);
}
#[test]
fn state_task_43_region_heartbeat_epoch_term_rejection() {
    let mut c = BasicCluster::new();
    let mut current = report(1, "", "", 3, 3);
    current.set_term(5);
    handle_region_heartbeat(&mut c, current.clone(), false).unwrap();
    let original = cached(&c, 1);
    for (v, cv, term) in [(2, 4, 5), (4, 2, 5), (3, 3, 4)] {
        let mut stale = report(1, "", "", v, cv);
        stale.set_term(term);
        let err = handle_region_heartbeat(&mut c, stale, false).unwrap_err();
        assert_eq!(err.current.epoch, current.meta().epoch);
        assert!(Arc::ptr_eq(&original, &cached(&c, 1)));
    }
    let mut unknown = current.clone();
    unknown.set_term(0);
    assert_eq!(
        handle_region_heartbeat(&mut c, unknown, false)
            .unwrap()
            .guide,
        RegionGuide::default()
    );
    let mut term_only = current;
    term_only.set_term(6);
    assert_eq!(
        handle_region_heartbeat(&mut c, term_only, false)
            .unwrap()
            .guide,
        RegionGuide::default()
    );
    assert_eq!(cached(&c, 1).term(), 5);
    check_indexes(&c, 1, 1);
}
#[test]
fn state_task_43_region_heartbeat_flow_size_and_leader() {
    let mut c = BasicCluster::new();
    let r = report(1, "", "", 2, 2);
    handle_region_heartbeat(&mut c, r.clone(), false).unwrap();
    let mut flow = r.clone();
    flow.stats_mut().read_bytes = 1000;
    let result = handle_region_heartbeat(&mut c, flow.clone(), false).unwrap();
    assert!(result.guide.save_cache && result.guide.need_sync && !result.guide.save_kv);
    assert_eq!(cached(&c, 1).stats().read_bytes, 1000);
    flow.stats_mut().approximate_size = 144;
    flow.stats_mut().approximate_keys = 144000;
    flow.stats_mut().approximate_ia_kv_size = 40;
    let result = handle_region_heartbeat(&mut c, flow, false).unwrap();
    assert!(result.guide.save_cache && !result.guide.need_sync && !result.guide.save_kv);
    assert_eq!(cached(&c, 1).stats().approximate_ia_kv_size, 40);
    check_indexes(&c, 1, 144);
    let changed = cached(&c, 1).clone_with([RegionOption::Leader(None)]);
    assert!(
        handle_region_heartbeat(&mut c, changed, false)
            .unwrap()
            .guide
            .need_sync
    );
    assert_eq!(c.regions.store_stats(1).leader, 0);
    assert_eq!(c.regions.store_stats(1).region, 1);
}
#[test]
fn state_task_43_region_heartbeat_prepared_overlap_recheck() {
    let mut c = BasicCluster::new();
    handle_region_heartbeat(&mut c, report(1, "a", "m", 2, 2), false).unwrap();
    handle_region_heartbeat(&mut c, report(2, "m", "z", 2, 2), false).unwrap();
    let delayed = prepare_region_heartbeat(&c, report(1, "a", "m", 3, 2), false).unwrap();
    let merged = handle_region_heartbeat(&mut c, report(2, "a", "z", 4, 2), false).unwrap();
    assert_eq!(
        merged
            .overlaps
            .iter()
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        vec![1]
    );
    let err = delayed.commit(&mut c).unwrap_err();
    assert_eq!(err.current.id, 2);
    assert_eq!(c.regions.tree_len(), 1);
    assert!(c.regions.get_region(1).is_none());
    check_indexes(&c, 2, 1);
}
#[test]
fn state_task_43_region_heartbeat_same_id_recheck_and_last_write() {
    let mut c = BasicCluster::new();
    handle_region_heartbeat(&mut c, report(1, "", "", 2, 2), false).unwrap();
    let delayed = prepare_region_heartbeat(&c, report(1, "", "", 3, 2), false).unwrap();
    handle_region_heartbeat(&mut c, report(1, "", "", 100, 2), false).unwrap();
    assert!(delayed.commit(&mut c).is_err());
    let base = (*cached(&c, 1)).clone();
    let mut a = base.clone();
    a.stats_mut().read_bytes = 1000;
    let mut b = base;
    b.stats_mut().read_bytes = 2000;
    let a = prepare_region_heartbeat(&c, a, false).unwrap();
    let b = prepare_region_heartbeat(&c, b, false).unwrap();
    b.commit(&mut c).unwrap();
    a.commit(&mut c).unwrap();
    assert_eq!(cached(&c, 1).stats().read_bytes, 1000);
    check_indexes(&c, 1, 1);
}
#[test]
fn state_task_43_region_heartbeat_duplicate_repairs_subtree() {
    let mut c = BasicCluster::new();
    let mut r = report(1, "", "", 2, 2);
    r.stats_mut().approximate_size = 5;
    c.regions
        .check_and_put_root_tree(Arc::new(r.clone()))
        .unwrap();
    assert_eq!(c.regions.store_stats(1).region, 0);
    let result = handle_region_heartbeat(&mut c, r, false).unwrap();
    assert_eq!(result.guide, RegionGuide::default());
    check_indexes(&c, 1, 5);
}

#[test]
fn state_task_43_region_heartbeat_concurrent_precheck_merge_commit() {
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;
    let c = Mutex::new(BasicCluster::new());
    handle_region_heartbeat(&mut c.lock().unwrap(), report(1, "a", "m", 2, 2), false).unwrap();
    handle_region_heartbeat(&mut c.lock().unwrap(), report(2, "m", "z", 2, 2), false).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let c = &c;
        let delayed = scope.spawn(move || {
            let prepared =
                prepare_region_heartbeat(&c.lock().unwrap(), report(1, "a", "m", 3, 2), false)
                    .unwrap();
            ready_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            prepared.commit(&mut c.lock().unwrap())
        });
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle_region_heartbeat(&mut c.lock().unwrap(), report(2, "a", "z", 4, 2), false).unwrap();
        resume_tx.send(()).unwrap();
        assert_eq!(delayed.join().unwrap().unwrap_err().current.id, 2);
    });
    let c = c.into_inner().unwrap();
    assert_eq!(c.regions.tree_len(), 1);
    check_indexes(&c, 2, 1);
}

#[test]
fn state_task_43_region_heartbeat_delayed_duplicate_cannot_resurrect() {
    let mut c = BasicCluster::new();
    let r = report(1, "a", "z", 2, 2);
    handle_region_heartbeat(&mut c, r.clone(), false).unwrap();
    let duplicate = prepare_region_heartbeat(&c, r, false).unwrap();
    handle_region_heartbeat(&mut c, report(2, "a", "z", 3, 2), false).unwrap();
    let result = duplicate.commit(&mut c).unwrap();
    assert_eq!(result.guide, RegionGuide::default());
    assert!(c.regions.get_region(1).is_none());
    assert_eq!(c.regions.tree_len(), 1);
    check_indexes(&c, 2, 1);
}

#[test]
fn state_task_43_region_heartbeat_membership_and_recovery() {
    let mut c = BasicCluster::new();
    let p = Peer {
        id: 12,
        store_id: 2,
        ..Default::default()
    };
    let r = report(1, "a", "z", 5, 5);
    handle_region_heartbeat(&mut c, r.clone(), false).unwrap();
    let mut r = r.clone_with([
        RegionOption::Peers(vec![r.leader().unwrap().clone(), p.clone()]),
        RegionOption::IncConfVer,
    ]);
    let result = handle_region_heartbeat(&mut c, r.clone(), false).unwrap();
    assert!(result.guide.save_kv && result.guide.save_cache);
    assert_eq!(c.regions.store_stats(2).region, 1);
    r = r.clone_with([
        RegionOption::PendingPeers(vec![p.clone()]),
        RegionOption::DownPeers(vec![DownPeer {
            peer: p.clone(),
            down_seconds: 42,
        }]),
    ]);
    let result = handle_region_heartbeat(&mut c, r, false).unwrap();
    assert!(result.guide.need_sync && !result.guide.save_kv);
    assert_eq!(c.regions.store_stats(2).pending, 1);
    // Existing core recovery rule: partial-range epoch (1,1) bypasses stale checks.
    handle_region_heartbeat(&mut c, report(1, "a", "z", 1, 1), false).unwrap();
    assert_eq!(cached(&c, 1).meta().epoch.unwrap().version, 1);
    assert_eq!(c.regions.store_stats(2).region, 0);
    assert_eq!(c.regions.store_stats(2).pending, 0);
    check_indexes(&c, 1, 1);
}
