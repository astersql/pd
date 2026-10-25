// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{ChangePeer, ChangePeerV2, ChangeType, Operation};
use astersql_pd_storelimit::Type;
use std::sync::Arc;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_019.json"
    ))
    .unwrap()
}
fn peer(role: i32, id: u64, w: bool) -> Peer {
    Peer {
        id,
        store_id: 2,
        role: match role {
            0 => PeerRole::Voter,
            1 => PeerRole::Learner,
            2 => PeerRole::IncomingVoter,
            3 => PeerRole::DemotingVoter,
            n => PeerRole::Unknown(n),
        },
        is_witness: w,
    }
}
fn region(peers: Vec<Peer>, pending: Vec<Peer>) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            peers,
            ..Default::default()
        },
        None,
        vec![],
        pending,
    )
}
#[test]
fn task_019_behavior_cases() {
    for c in fixture()["completion"].as_array().unwrap() {
        let peers = c[1]
            .as_i64()
            .map(|n| {
                vec![peer(
                    n as i32,
                    c[2].as_u64().unwrap(),
                    c[3].as_bool().unwrap(),
                )]
            })
            .unwrap_or_default();
        let pending = c[4]
            .as_i64()
            .map(|n| vec![peer(n as i32, 12, false)])
            .unwrap_or_default();
        let r = region(peers, pending);
        let a = AddPeer {
            to_store: 2,
            peer_id: 12,
            ..Default::default()
        };
        let l = AddLearner {
            to_store: 2,
            peer_id: 12,
            ..Default::default()
        };
        let p = PromoteLearner {
            to_store: 2,
            peer_id: 12,
            ..Default::default()
        };
        assert_eq!(
            (a.is_finish(&r), a.conf_ver_changed(&r)),
            (c[5].as_bool().unwrap(), c[6].as_u64().unwrap()),
            "{c}"
        );
        assert_eq!(
            (l.is_finish(&r), l.conf_ver_changed(&r)),
            (c[7].as_bool().unwrap(), c[8].as_u64().unwrap()),
            "{c}"
        );
        assert_eq!(
            (p.is_finish(&r), p.conf_ver_changed(&r)),
            (c[9].as_bool().unwrap(), c[10].as_u64().unwrap()),
            "{c}"
        );
        for v2 in [false, true] {
            assert_eq!(a.get_cmd(&r, v2).is_none(), !c[1].is_null());
            assert_eq!(l.get_cmd(&r, v2).is_none(), !c[1].is_null());
            assert!(p.get_cmd(&r, v2).change_peer.is_some() != v2);
        }
    }
}
#[test]
fn task_019_commands_and_costs() {
    let r = region(vec![], vec![]);
    for w in [false, true] {
        for v2 in [false, true] {
            let a = AddPeer {
                to_store: 2,
                peer_id: 12,
                is_witness: w,
                ..Default::default()
            };
            let l = AddLearner {
                to_store: 2,
                peer_id: 12,
                is_witness: w,
                ..Default::default()
            };
            let p = PromoteLearner {
                to_store: 2,
                peer_id: 12,
                is_witness: w,
            };
            for (cmd, role, typ) in [
                (a.get_cmd(&r, v2).unwrap(), 0, ChangeType::AddNode),
                (l.get_cmd(&r, v2).unwrap(), 1, ChangeType::AddLearnerNode),
                (p.get_cmd(&r, v2), 0, ChangeType::AddNode),
            ] {
                let change = ChangePeer {
                    peer: Some(peer(role, 12, w)),
                    change_type: typ,
                };
                let expected = if v2 {
                    Operation {
                        change_peer_v2: Some(ChangePeerV2 {
                            changes: vec![Some(change)],
                        }),
                        ..Default::default()
                    }
                } else {
                    Operation {
                        change_peer: Some(change),
                        ..Default::default()
                    }
                };
                assert_eq!(cmd, expected);
            }
            assert_eq!(
                a.to_string(),
                format!("add {}peer 12 on store 2", if w { "witness " } else { "" })
            );
            assert_eq!(
                l.to_string(),
                format!(
                    "add {}learner peer 12 on store 2",
                    if w { "witness " } else { "" }
                )
            );
            assert_eq!(
                p.to_string(),
                format!(
                    "promote {}learner peer 12 on store 2 to voter",
                    if w { "witness " } else { "" }
                )
            );
        }
    }
    for c in fixture()["costs"].as_array().unwrap() {
        for light in [false, true] {
            for w in [false, true] {
                for send in [0, 2, 3] {
                    let mut r = region(vec![], vec![]);
                    let size = c[0].as_i64().unwrap();
                    r.stats_mut().approximate_size = size;
                    r.stats_mut().approximate_keys = c[1].as_i64().unwrap();
                    let a = AddPeer {
                        to_store: 2,
                        peer_id: 12,
                        is_light_weight: light,
                        is_witness: w,
                    };
                    let l = AddLearner {
                        to_store: 2,
                        peer_id: 12,
                        send_store: send,
                        is_light_weight: light,
                        is_witness: w,
                    };
                    for learner in [false, true] {
                        let inf = OpInfluence::new();
                        if learner {
                            l.influence(&inf, &r)
                        } else {
                            a.influence(&inf, &r)
                        };
                        let s = inf.get_store_influence(2);
                        assert_eq!(
                            (
                                s.region_size,
                                s.region_count,
                                s.witness_count,
                                s.leader_count,
                                s.leader_size
                            ),
                            (if w { 0 } else { size }, 1, i64::from(w), 0, 0)
                        );
                        assert_eq!(
                            s.get_step_cost(Type::ADD_PEER),
                            if light || w {
                                0
                            } else {
                                c[2].as_i64().unwrap()
                            }
                        );
                        let charged = learner && !light && !w && send != 0;
                        assert_eq!(
                            inf.snapshot().len(),
                            if charged && send != 2 { 2 } else { 1 }
                        );
                        if charged {
                            assert_eq!(
                                inf.get_store_influence(send)
                                    .get_step_cost(Type::SEND_SNAPSHOT),
                                size
                            );
                        }
                        let before = inf.snapshot();
                        PromoteLearner::default().influence(&inf, &r);
                        assert_eq!(inf.snapshot(), before);
                    }
                }
            }
        }
    }
    for (size, slow, fast) in [
        (-1, 600, 60),
        (0, 600, 60),
        (100, 600, 60),
        (101, 606, 60),
        (102, 612, 61),
        (200, 1200, 120),
        (i64::MAX, 600, 60),
    ] {
        assert_eq!(AddPeer::default().timeout(size).as_secs(), slow);
        assert_eq!(AddLearner::default().timeout(size).as_secs(), slow);
        assert_eq!(PromoteLearner::default().timeout(size).as_secs(), fast);
    }
}
#[test]
fn task_019_validation_faults() {
    let mut cfg = ProviderSnapshot::<String>::default();
    cfg.schedule.max_store_down_time = "30m".parse().unwrap();
    for (exists, elapsed, health, err) in [
        (false, 0, true, Some("target store does not exist")),
        (true, 600_000_000_000, true, None),
        (
            true,
            600_000_000_001,
            true,
            Some("target store is unhealthy"),
        ),
        (true, 1_800_000_000_000, false, None),
        (true, 1_800_000_000_001, false, Some("target store is down")),
        (true, -1, true, None),
    ] {
        let cluster = BasicCluster::new();
        if exists {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id: 2,
                last_heartbeat: 2_000_000_000_000 - elapsed,
                ..Default::default()
            })));
        }
        for (id, role) in [(12, 0), (12, 1), (99, 0)] {
            let r = region(vec![peer(role, id, false)], vec![]);
            let a = AddPeer {
                to_store: 2,
                peer_id: 12,
                ..Default::default()
            };
            let l = AddLearner {
                to_store: 2,
                peer_id: 12,
                ..Default::default()
            };
            let conflict = "peer 99 has already existed in store 2, the timeout is trying to add peer 12 on the same store";
            assert_eq!(
                a.check_in_progress(&cluster, &cfg, &r, health, 2_000_000_000_000)
                    .err()
                    .map(|e| e.to_string())
                    .as_deref(),
                err.or(if id == 99 { Some(conflict) } else { None })
            );
            assert_eq!(
                l.check_in_progress(&cluster, &cfg, &r, health, 2_000_000_000_000)
                    .err()
                    .map(|e| e.to_string())
                    .as_deref(),
                err.or(if id == 99 {
                    Some(conflict)
                } else if role == 0 {
                    Some("peer already is a voter")
                } else {
                    None
                })
            );
            assert_eq!(
                PromoteLearner {
                    to_store: 2,
                    peer_id: 12,
                    is_witness: false
                }
                .check_in_progress(&cluster, &cfg, &r, health, 0)
                .is_ok(),
                id == 12
            );
        }
    }
    let cluster = BasicCluster::new();
    let r = region(vec![], vec![]);
    assert!(
        PromoteLearner {
            to_store: 2,
            peer_id: 12,
            is_witness: false
        }
        .check_in_progress(&cluster, &cfg, &r, true, 0)
        .is_err()
    );
    // Go's nil-safe peer getters make ID zero equal to an absent peer.
    assert_eq!(AddPeer::default().conf_ver_changed(&r), 1);
    assert_eq!(AddLearner::default().conf_ver_changed(&r), 1);
    assert_eq!(PromoteLearner::default().conf_ver_changed(&r), 1);
    assert!(!PromoteLearner::default().is_finish(&r));
    assert!(
        PromoteLearner::default()
            .check_in_progress(&cluster, &cfg, &r, true, 0)
            .is_ok()
    );
}

#[test]
fn task_019_heartbeat_and_boundaries() {
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        ..Default::default()
    })));
    let cfg = ProviderSnapshot::<String>::default();
    let empty = region(vec![], vec![]);
    let a = AddPeer {
        to_store: 2,
        peer_id: 12,
        is_witness: true,
        is_light_weight: false,
    };
    let l = AddLearner {
        to_store: 2,
        peer_id: 12,
        is_witness: true,
        is_light_weight: false,
        send_store: 3,
    };
    assert_eq!(
        a.check_in_progress(&cluster, &cfg, &empty, false, 0),
        Ok(())
    );
    assert_eq!(
        l.check_in_progress(&cluster, &cfg, &empty, false, 0),
        Ok(())
    );
    assert!(a.get_cmd(&empty, false).is_some());
    assert!(!a.is_finish(&empty));
    let pending = region(vec![peer(0, 12, true)], vec![peer(0, 12, true)]);
    assert!(!a.is_finish(&pending));
    assert!(a.get_cmd(&pending, false).is_none());
    let ready = region(vec![peer(0, 12, true)], vec![]);
    assert!(a.is_finish(&ready));
    let learner = region(vec![peer(1, 12, true)], vec![]);
    assert!(l.is_finish(&learner));
    for role in [0, 2, 3, 9] {
        let r = region(vec![peer(role, 12, true)], vec![]);
        assert_eq!(
            l.check_in_progress(&cluster, &cfg, &r, false, 0),
            Err(PeerStepError::AlreadyVoter)
        );
        assert_eq!(a.check_in_progress(&cluster, &cfg, &r, false, 0), Ok(()));
    }
    let wrong = region(vec![peer(1, 99, false)], vec![]);
    assert_eq!(
        l.check_in_progress(&cluster, &cfg, &wrong, false, 0),
        Err(PeerStepError::ExistingPeer {
            existing: 99,
            store: 2,
            expected: 12
        })
    );
    assert_eq!(
        PromoteLearner {
            to_store: 2,
            peer_id: 12,
            is_witness: false
        }
        .check_in_progress(&cluster, &cfg, &empty, false, 0),
        Err(PeerStepError::MissingPeer)
    );
    let inf = OpInfluence::new();
    inf.update_store(2, |s| {
        s.region_count = i64::MAX;
        s.region_size = i64::MAX;
        s.witness_count = i64::MAX;
    });
    let mut r = ready;
    r.stats_mut().approximate_size = 1;
    a.influence(&inf, &r);
    assert_eq!(inf.get_store_influence(2).witness_count, i64::MIN);
    assert_eq!(inf.get_store_influence(2).region_count, i64::MIN);
    AddPeer {
        is_witness: false,
        ..a
    }
    .influence(&inf, &r);
    assert_eq!(inf.get_store_influence(2).region_size, i64::MIN);
}
