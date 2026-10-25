// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{ChangePeer, ChangePeerV2, ChangeType, Operation};
fn peer(store_id: u64, id: u64, role: i64, witness: bool) -> Peer {
    Peer {
        store_id,
        id,
        role: match role {
            0 => PeerRole::Voter,
            1 => PeerRole::Learner,
            2 => PeerRole::IncomingVoter,
            3 => PeerRole::DemotingVoter,
            n => PeerRole::Unknown(n as i32),
        },
        is_witness: witness,
    }
}
fn region(peers: Vec<Peer>, leader: u64, pending: bool) -> RegionInfo {
    let l = peers.iter().find(|p| p.store_id == leader).cloned();
    let pend = if pending { peers.clone() } else { vec![] };
    RegionInfo::new(
        RegionMeta {
            peers,
            ..Default::default()
        },
        l,
        vec![],
        pend,
    )
}
fn steps() -> (ChangePeerV2Enter, ChangePeerV2Leave) {
    let p = vec![PromoteLearner {
        to_store: 1,
        peer_id: 11,
        is_witness: true,
    }];
    let d = vec![DemoteVoter {
        to_store: 2,
        peer_id: 12,
        is_witness: false,
    }];
    (
        ChangePeerV2Enter {
            promote_learners: p.clone(),
            demote_voters: d.clone(),
        },
        ChangePeerV2Leave {
            promote_learners: p,
            demote_voters: d,
        },
    )
}
#[test]
fn task_022_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_022.json"
    ))
    .unwrap();
    for c in f["cases"].as_array().unwrap() {
        let (mut e, mut l) = steps();
        e.demote_voters[0].is_witness = c["witness"].as_bool().unwrap();
        l.demote_voters = e.demote_voters.clone();
        let mut peers = vec![];
        for (key, store, id) in [("p", 1, "pid"), ("d", 2, "did")] {
            if let Some(role) = c[key].as_i64() {
                peers.push(peer(store, c[id].as_u64().unwrap(), role, false));
            }
        }
        if let Some(role) = c["extra"].as_i64() {
            peers.push(peer(3, 13, role, false));
        }
        for pending in [false, true] {
            let r = region(peers.clone(), c["leader"].as_u64().unwrap(), pending);
            let cfg = ProviderSnapshot::<()>::default();
            let cluster = BasicCluster::new();
            for (key, delta, finish, err) in [
                (
                    "enter",
                    e.conf_ver_changed(&r),
                    e.is_finish(&r),
                    e.check_in_progress(&cluster, &cfg, &r, true, 0),
                ),
                (
                    "leave",
                    l.conf_ver_changed(&r),
                    l.is_finish(&r),
                    l.check_in_progress(&cluster, &cfg, &r, true, 0),
                ),
            ] {
                assert_eq!(
                    delta,
                    c[key][0].as_u64().unwrap(),
                    "{} {key} confver",
                    c["name"]
                );
                assert_eq!(
                    finish,
                    c[key][1].as_bool().unwrap(),
                    "{} {key} finish",
                    c["name"]
                );
                assert_eq!(
                    err.err().map(|e| e.to_string()),
                    c[key][2].as_str().map(str::to_owned),
                    "{} {key} error",
                    c["name"]
                );
            }
        }
    }
}
#[test]
fn task_022_commands_empty_single_and_cost() {
    let (mut e, mut l) = steps();
    let r = region(vec![], 0, false);
    e.promote_learners.push(e.promote_learners[0].clone());
    let expected = Operation {
        change_peer_v2: Some(ChangePeerV2 {
            changes: vec![
                Some(ChangePeer {
                    peer: Some(peer(1, 11, 0, true)),
                    change_type: ChangeType::AddNode,
                }),
                Some(ChangePeer {
                    peer: Some(peer(1, 11, 0, true)),
                    change_type: ChangeType::AddNode,
                }),
                Some(ChangePeer {
                    peer: Some(peer(2, 12, 1, false)),
                    change_type: ChangeType::AddLearnerNode,
                }),
            ],
        }),
        ..Default::default()
    };
    assert_eq!(e.get_cmd(&r, true), Some(expected));
    assert_eq!(e.get_cmd(&r, false), None);
    let empty = Some(Operation {
        change_peer_v2: Some(ChangePeerV2 { changes: vec![] }),
        ..Default::default()
    });
    assert_eq!(l.get_cmd(&r, true), empty);
    assert_eq!(l.get_cmd(&r, false), None);
    for size in [-1, 0, 100, 101, 1_000, i64::MAX] {
        let fast = TransferLeader::default().timeout(size).as_nanos() as i64;
        assert_eq!(e.timeout_nanos(size), fast.wrapping_mul(4));
        assert_eq!(l.timeout_nanos(size), fast.wrapping_mul(3));
        assert_eq!(
            e.timeout(size).as_nanos(),
            e.timeout_nanos(size) as u64 as u128
        );
        assert_eq!(
            l.timeout(size).as_nanos(),
            l.timeout_nanos(size) as u64 as u128
        );
    }
    let inf = OpInfluence::default();
    inf.update_store(99, |s| s.region_size = 123);
    let before = inf.snapshot();
    e.influence(&inf, &r);
    l.influence(&inf, &r);
    assert_eq!(inf.snapshot(), before);
    let suffix =
        ", promote learner peer 11 on store 1 to voter, demote voter peer 12 on store 2 to learner";
    assert_eq!(l.to_string(), format!("leave joint state{suffix}"));
    e.promote_learners.pop();
    assert_eq!(e.to_string(), format!("use joint consensus{suffix}"));
    // Each single-member operation may skip directly to its stable role.
    for promotion in [false, true] {
        let mut single = e.clone();
        if promotion {
            single.demote_voters.clear();
        } else {
            single.promote_learners.clear();
        }
        let sr = region(
            vec![if promotion {
                peer(1, 11, 0, true)
            } else {
                peer(2, 12, 1, false)
            }],
            0,
            false,
        );
        assert!(single.is_finish(&sr));
        assert_eq!(single.conf_ver_changed(&sr), 1);
    }
    e = ChangePeerV2Enter::default();
    l = ChangePeerV2Leave::default();
    assert!(e.is_finish(&r));
    assert!(l.is_finish(&r));
    assert_eq!(e.conf_ver_changed(&r), 0);
    assert_eq!(l.conf_ver_changed(&r), 0);
    assert_eq!(e.get_cmd(&r, true), empty);
    let joint = region(vec![peer(9, 99, 2, false)], 0, false);
    assert!(e.is_finish(&joint));
    assert!(!l.is_finish(&joint));
    assert!(
        e.check_in_progress(
            &BasicCluster::new(),
            &ProviderSnapshot::<()>::default(),
            &joint,
            true,
            0
        )
        .is_ok()
    );
    // Go nil getters return ID 0 and Voter: preserve the zero-ID promotion edge.
    e.promote_learners.push(PromoteLearner {
        to_store: 1,
        peer_id: 0,
        is_witness: false,
    });
    l.promote_learners = e.promote_learners.clone();
    assert!(e.is_finish(&r));
    assert_eq!(e.conf_ver_changed(&r), 1);
    assert!(!l.is_finish(&r));
    assert_eq!(l.conf_ver_changed(&r), 1);
}
