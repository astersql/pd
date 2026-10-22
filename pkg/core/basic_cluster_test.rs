// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;

fn cluster() -> BasicCluster {
    let mut bc = BasicCluster::new();
    for id in 0..=4 {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            node_state: if id == 4 {
                NodeState::Removed
            } else {
                NodeState::Serving
            },
            ..Default::default()
        });
        store.set_region_count(99);
        bc.stores.put_store(Arc::new(store));
    }
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 3 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: id == 2,
        })
        .collect();
    for (id, size) in [(1, 5), (2, 8)] {
        let mut r = RegionInfo::new(
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
        r.set_heartbeat_stats(
            RegionHeartbeatStats {
                approximate_size: size * 1048576,
                ..Default::default()
            },
            0,
        );
        let r = Arc::new(r);
        let (old, overlaps, changed) = bc.regions.set_region(r.clone());
        bc.regions
            .update_sub_tree(r, old.as_deref(), &overlaps, changed);
    }
    bc
}
fn counts(s: &StoreInfo) -> Vec<i64> {
    vec![
        s.leader_count(),
        s.region_count(),
        s.witness_count(),
        s.learner_count(),
        s.pending_peer_count(),
        s.leader_size(),
        s.region_size(),
    ]
}
#[test]
fn core_task_57_status_and_snapshot() {
    let bc = cluster();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_57.json")).unwrap();
    let old = bc.stores.get_store(1).unwrap();
    bc.update_all_store_status();
    for id in 0..=3 {
        assert_eq!(
            serde_json::to_value(counts(&bc.stores.get_store(id).unwrap())).unwrap(),
            fixture["expected"][id.to_string()]
        );
    }
    assert_eq!(old.region_count(), 99);
    assert_eq!(bc.stores.get_store(4).unwrap().region_count(), 99);
    bc.update_store_status(4);
    assert_eq!(bc.stores.get_store(4).unwrap().region_count(), 0);
    bc.update_store_status(999);
    assert_eq!(bc.stores.store_count(), 5);
}
#[test]
fn core_task_57_removal_clears_status() {
    let mut bc = cluster();
    bc.update_all_store_status();
    bc.regions.remove_region_if_exist(1);
    bc.update_store_status(1);
    assert_eq!(
        counts(&bc.stores.get_store(1).unwrap()),
        vec![1, 1, 0, 0, 0, 8, 8]
    );
    bc.regions.remove_region_if_exist(2);
    bc.update_all_store_status();
    assert_eq!(counts(&bc.stores.get_store(3).unwrap()), vec![0; 7]);
}
#[test]
fn core_task_57_leader_lookup_absent_paths() {
    let mut bc = cluster();
    assert_eq!(bc.get_leader_store_by_region_id(1).unwrap().id(), 1);
    assert!(bc.get_leader_store_by_region_id(999).is_none());
    bc.stores.delete_store(1);
    assert!(bc.get_leader_store_by_region_id(1).is_none());
    bc.regions.set_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 3,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )));
    assert!(bc.get_leader_store_by_region_id(3).is_none()); // store zero exists
}
