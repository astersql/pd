// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_55.json")).unwrap()
}
fn peer(id: u64, learner: bool, witness: bool) -> Peer {
    Peer {
        id,
        store_id: id,
        role: if learner {
            PeerRole::Learner
        } else {
            PeerRole::Voter
        },
        is_witness: witness,
    }
}
fn region() -> Arc<RegionInfo> {
    let f = fixture();
    Arc::new(RegionInfo::new(
        RegionMeta {
            id: f["region"]["id"].as_u64().unwrap(),
            key_range: KeyRange {
                start_key: f["region"]["start"].as_str().unwrap().into(),
                end_key: f["region"]["end"].as_str().unwrap().into(),
            },
            peers: vec![
                peer(1, false, false),
                peer(2, false, true),
                peer(3, true, false),
            ],
            ..Default::default()
        },
        Some(peer(1, false, false)),
        vec![],
        vec![peer(3, true, false)],
    ))
}
fn put(r: &mut RegionsInfo, v: Arc<RegionInfo>) {
    let (old, overlaps, changed) = r.set_region(v.clone());
    r.update_sub_tree(v, old.as_deref(), &overlaps, changed);
}
fn sample(r: &RegionsInfo, role: &str, store: u64, ranges: &[KeyRange]) -> Vec<Arc<RegionInfo>> {
    match role {
        "leader" => r.rand_leader_regions(store, ranges, |_| 0, |_| {}),
        "follower" => r.rand_follower_regions(store, ranges, |_| 0, |_| {}),
        "learner" => r.rand_learner_regions(store, ranges, |_| 0, |_| {}),
        "witness" => r.rand_witness_regions(store, ranges, |_| 0, |_| {}),
        "pending" => r.rand_pending_regions(store, ranges, |_| 0, |_| {}),
        _ => panic!("unknown fixture role"),
    }
}
#[test]
fn core_task_55_roles_ranges_and_absence() {
    let f = fixture();
    let mut r = RegionsInfo::new();
    for pair in f["roles"].as_array().unwrap() {
        assert!(
            sample(
                &r,
                pair[0].as_str().unwrap(),
                pair[1].as_u64().unwrap(),
                &[]
            )
            .is_empty()
        );
    }
    put(&mut r, region());
    for pair in f["roles"].as_array().unwrap() {
        let role = pair[0].as_str().unwrap();
        let store = pair[1].as_u64().unwrap();
        assert!(sample(&r, role, 999, &[]).is_empty());
        for case in f["range_cases"].as_array().unwrap() {
            let ranges = [KeyRange {
                start_key: case[0].as_str().unwrap().into(),
                end_key: case[1].as_str().unwrap().into(),
            }];
            let result = sample(&r, role, store, &ranges);
            assert_eq!(result.len(), case[2].as_u64().unwrap() as usize);
            assert!(result.iter().all(|v| v.meta().id == 55));
        }
        assert_eq!(
            sample(&r, role, store, &[]).len(),
            f["draw_count"].as_u64().unwrap() as usize
        );
    }
}
#[test]
fn core_task_55_role_changes_remove_old_candidates() {
    let mut r = RegionsInfo::new();
    put(&mut r, region());
    let old = region();
    let replacement = Arc::new(RegionInfo::new(
        RegionMeta {
            peers: vec![peer(4, false, false)],
            ..old.meta().clone()
        },
        Some(peer(4, false, false)),
        vec![],
        vec![],
    ));
    put(&mut r, replacement.clone());
    for pair in fixture()["roles"].as_array().unwrap() {
        assert!(
            sample(
                &r,
                pair[0].as_str().unwrap(),
                pair[1].as_u64().unwrap(),
                &[]
            )
            .is_empty()
        );
    }
    let selected = sample(&r, "leader", 4, &[]);
    assert_eq!(selected.len(), 10);
    assert!(selected.iter().all(|v| Arc::ptr_eq(v, &replacement)));
}
#[test]
fn core_task_55_witness_leader() {
    assert!(!need_transfer_witness_leader(None));
    let r = region();
    assert!(!need_transfer_witness_leader(Some(&r)));
    for (leader, expected) in [(None, false), (Some(peer(99, false, true)), true)] {
        let copy = r.clone_with([RegionOption::Leader(leader)]);
        assert_eq!(need_transfer_witness_leader(Some(&copy)), expected);
    }
}

#[test]
fn core_task_55_distinct_candidates_and_injected_randomness() {
    let mut r = RegionsInfo::new();
    put(&mut r, region());
    let second = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 56,
            key_range: KeyRange {
                start_key: b"f".to_vec(),
                end_key: b"z".to_vec(),
            },
            peers: vec![peer(1, false, false), peer(2, false, false)],
            ..Default::default()
        },
        Some(peer(1, false, false)),
        vec![],
        vec![],
    ));
    put(&mut r, second);
    let mut draw = 0;
    let result = r.rand_follower_regions(
        2,
        &[],
        |bound| {
            let i = draw % bound;
            draw += 1;
            i
        },
        |_| {},
    );
    assert_eq!(
        result.iter().map(|v| v.meta().id).collect::<Vec<_>>(),
        vec![55, 56, 55, 56, 55, 56, 55, 56, 55, 56]
    );
    assert!(
        r.rand_witness_regions(2, &[], |bound| bound - 1, |_| {})
            .iter()
            .all(|v| v.meta().id == 55)
    );
    assert!(r.rand_leader_regions(2, &[], |_| 0, |_| {}).is_empty());
    let ranges = [
        KeyRange {
            start_key: b"x".to_vec(),
            end_key: b"y".to_vec(),
        },
        KeyRange {
            start_key: b"f".to_vec(),
            end_key: b"z".to_vec(),
        },
    ];
    let selected = r.rand_leader_regions(1, &ranges, |_| 0, |order| order.reverse());
    assert_eq!(selected.len(), 10);
    assert!(selected.iter().all(|v| v.meta().id == 56));
}
