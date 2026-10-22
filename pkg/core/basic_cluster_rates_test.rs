// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;

fn assert_rates(bc: &BasicCluster, expected: &serde_json::Value) {
    for (name, (ids, bytes, keys)) in [
        ("leader", bc.get_stores_leader_write_rate()),
        ("all", bc.get_stores_write_rate()),
    ] {
        assert_eq!(ids.len(), bytes.len());
        assert_eq!(ids.len(), keys.len());
        let mut rows: Vec<_> = ids
            .into_iter()
            .zip(bytes)
            .zip(keys)
            .map(|((id, b), k)| (id, b, k))
            .collect();
        rows.sort_by_key(|r| r.0);
        assert_eq!(serde_json::to_value(rows).unwrap(), expected[name]);
    }
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_58.json")).unwrap()
}
#[test]
fn core_task_58_empty_and_missing_store() {
    let mut bc = BasicCluster::new();
    assert_rates(&bc, &fixture()["empty"]);
    let peer = Peer {
        id: 1,
        store_id: 99,
        ..Default::default()
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            written_bytes: 100,
            written_keys: 20,
            ..Default::default()
        },
        0,
    );
    r.set_report_interval(0, 5);
    let r = Arc::new(r);
    let (old, overlaps, changed) = bc.regions.set_region(r.clone());
    bc.regions
        .update_sub_tree(r, old.as_deref(), &overlaps, changed);
    assert_rates(&bc, &fixture()["empty"]);
    bc.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 7,
        ..Default::default()
    })));
    assert_rates(&bc, &fixture()["missing_index"]);
}
#[test]
fn core_task_58_alignment_roles_removed_and_deletion() {
    let mut bc = BasicCluster::new();
    for id in [4, 2, 7, 1, 3] {
        bc.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            node_state: if id == 4 {
                NodeState::Removed
            } else {
                NodeState::Serving
            },
            ..Default::default()
        })));
    }
    for (id, leader, bytes, keys) in [(1, 1, 100, 20), (2, 2, 50, 10)] {
        let peers: Vec<_> = (1..=4)
            .map(|store_id| Peer {
                id: store_id,
                store_id,
                role: if store_id == 3 {
                    PeerRole::Learner
                } else {
                    PeerRole::Voter
                },
                is_witness: store_id == 4,
            })
            .collect();
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
            Some(peers[leader - 1].clone()),
            vec![],
            vec![],
        );
        r.set_heartbeat_stats(
            RegionHeartbeatStats {
                written_bytes: bytes,
                written_keys: keys,
                ..Default::default()
            },
            0,
        );
        r.set_report_interval(0, 5);
        let r = Arc::new(r);
        let (old, overlaps, changed) = bc.regions.set_region(r.clone());
        bc.regions
            .update_sub_tree(r, old.as_deref(), &overlaps, changed);
    }
    assert_rates(&bc, &fixture()["populated"]);
    bc.stores.delete_store(1);
    assert_rates(&bc, &fixture()["deleted_store"]);
    bc.regions.remove_region_if_exist(1);
    bc.regions.remove_region_if_exist(2);
    assert_rates(&bc, &fixture()["deleted_regions"]);
}
