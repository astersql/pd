// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;

fn region(
    id: u64,
    start: &str,
    end: &str,
    leader: u64,
    peers: Vec<Peer>,
    pending: Vec<Peer>,
) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.into(),
                end_key: end.into(),
            },
            peers,
            ..Default::default()
        },
        peers_leader(leader),
        vec![],
        pending,
    ))
}
fn peers_leader(id: u64) -> Option<Peer> {
    (id != 0).then(|| peer(id, false, false))
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
fn put(r: &mut RegionsInfo, incoming: Arc<RegionInfo>) {
    let (origin, overlaps, changed) = r.set_region(incoming.clone());
    r.update_sub_tree(incoming, origin.as_deref(), &overlaps, changed);
}
#[test]
fn core_task_47_roles_and_range_changes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_47.json")).unwrap();
    let mut r = RegionsInfo::new();
    put(
        &mut r,
        region(
            1,
            "a",
            "m",
            1,
            vec![
                peer(1, false, false),
                peer(2, false, true),
                peer(3, true, false),
            ],
            vec![peer(3, true, false)],
        ),
    );
    for (role, store) in [
        (RegionRole::Leader, 1),
        (RegionRole::Follower, 2),
        (RegionRole::Witness, 2),
        (RegionRole::Learner, 3),
        (RegionRole::Pending, 3),
    ] {
        assert_eq!(
            r.role_tree(role, store)
                .unwrap()
                .search(b"a")
                .unwrap()
                .meta()
                .id,
            1
        );
    }
    put(
        &mut r,
        region(
            1,
            "m",
            "z",
            2,
            vec![peer(2, false, false), peer(4, true, true)],
            vec![peer(2, false, false)],
        ),
    );
    for (role, store) in [
        (RegionRole::Leader, 1),
        (RegionRole::Follower, 2),
        (RegionRole::Witness, 2),
        (RegionRole::Learner, 3),
        (RegionRole::Pending, 3),
    ] {
        assert!(r.role_tree(role, store).unwrap().search(b"a").is_none());
    }
    for row in fixture["replacement"].as_array().unwrap() {
        let role = match row[0].as_str().unwrap() {
            "leader" => RegionRole::Leader,
            "learner" => RegionRole::Learner,
            "witness" => RegionRole::Witness,
            _ => RegionRole::Pending,
        };
        assert_eq!(
            r.role_tree(role, row[1].as_u64().unwrap())
                .unwrap()
                .search(b"m")
                .unwrap()
                .meta()
                .id,
            1
        );
    }
}
#[test]
fn core_task_47_overlap_fallback_and_empty_membership() {
    let mut r = RegionsInfo::new();
    put(
        &mut r,
        region(1, "a", "m", 1, vec![peer(1, false, false)], vec![]),
    );
    let incoming = region(2, "a", "z", 0, vec![], vec![]);
    r.set_region(incoming.clone());
    r.update_sub_tree(incoming, None, &[], true);
    assert!(
        r.role_tree(RegionRole::Leader, 1)
            .unwrap()
            .search(b"a")
            .is_none()
    );
    assert!(r.role_tree(RegionRole::Pending, 99).is_none());
}
#[test]
fn core_task_47_same_range_payload_and_nil_leader() {
    let mut r = RegionsInfo::new();
    let old = region(1, "", "", 0, vec![peer(1, false, false)], vec![]);
    put(&mut r, old.clone());
    let new = region(1, "", "", 0, vec![peer(1, false, false)], vec![]);
    put(&mut r, new.clone());
    let found = r
        .role_tree(RegionRole::Follower, 1)
        .unwrap()
        .search(b"z")
        .unwrap();
    assert!(Arc::ptr_eq(&new, &found));
    assert_eq!(old.get_ref(), 0);
    assert_eq!(new.get_ref(), 2);
}

#[test]
fn core_task_47_same_range_role_changes_and_explicit_overlap() {
    let mut r = RegionsInfo::new();
    put(
        &mut r,
        region(
            1,
            "a",
            "m",
            1,
            vec![peer(1, false, false), peer(2, false, false)],
            vec![peer(2, false, false)],
        ),
    );
    put(
        &mut r,
        region(
            1,
            "a",
            "m",
            2,
            vec![peer(1, true, true), peer(2, false, false)],
            vec![],
        ),
    );
    assert!(
        r.role_tree(RegionRole::Leader, 1)
            .unwrap()
            .search(b"a")
            .is_none()
    );
    assert!(
        r.role_tree(RegionRole::Follower, 2)
            .unwrap()
            .search(b"a")
            .is_none()
    );
    assert!(
        r.role_tree(RegionRole::Pending, 2)
            .unwrap()
            .search(b"a")
            .is_none()
    );
    for (role, store) in [
        (RegionRole::Leader, 2),
        (RegionRole::Learner, 1),
        (RegionRole::Witness, 1),
    ] {
        assert_eq!(
            r.role_tree(role, store)
                .unwrap()
                .search(b"a")
                .unwrap()
                .meta()
                .id,
            1
        );
    }
    put(
        &mut r,
        region(2, "a", "z", 3, vec![peer(3, false, false)], vec![]),
    );
    for (role, store) in [
        (RegionRole::Leader, 2),
        (RegionRole::Learner, 1),
        (RegionRole::Witness, 1),
    ] {
        assert!(r.role_tree(role, store).unwrap().search(b"a").is_none());
    }
    assert_eq!(
        r.role_tree(RegionRole::Leader, 3)
            .unwrap()
            .search(b"m")
            .unwrap()
            .meta()
            .id,
        2
    );
}
