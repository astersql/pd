// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{ChangePeer, ChangePeerV2, ChangeType, Operation};
use astersql_pd_storelimit::Type;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_020.json"
    ))
    .unwrap()
}
fn peer(role: i32, id: u64, witness: bool) -> Peer {
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
        is_witness: witness,
    }
}
fn region(peers: Vec<Peer>, leader: Option<Peer>) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            peers,
            ..Default::default()
        },
        leader,
        vec![],
        vec![],
    )
}
#[test]
fn task_020_behavior_cases() {
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
        let r = region(peers, None);
        let remove = RemovePeer {
            from_store: 2,
            peer_id: 12,
            ..Default::default()
        };
        let demote = DemoteVoter {
            to_store: 2,
            peer_id: 12,
            is_witness: false,
        };
        assert_eq!(
            (remove.is_finish(&r), remove.conf_ver_changed(&r)),
            (c[4].as_bool().unwrap(), c[5].as_u64().unwrap()),
            "{c}"
        );
        assert_eq!(
            (demote.is_finish(&r), demote.conf_ver_changed(&r)),
            (c[6].as_bool().unwrap(), c[7].as_u64().unwrap()),
            "{c}"
        );
        assert_eq!(
            DemoteVoter {
                is_witness: true,
                ..demote
            }
            .conf_ver_changed(&r),
            c[8].as_u64().unwrap(),
            "{c}"
        );
        assert_eq!(
            RemovePeer {
                peer_id: 0,
                ..remove
            }
            .conf_ver_changed(&r),
            u64::from(c[1].is_null() || c[2] == 0)
        );
    }
}
#[test]
fn task_020_commands_and_timeouts() {
    for role in [0, 1, 2, 3, 9] {
        for w in [false, true] {
            for present in [false, true] {
                let p = peer(role, 99, w);
                let r = region(if present { vec![p.clone()] } else { vec![] }, None);
                let remove = RemovePeer {
                    from_store: 2,
                    peer_id: 12,
                    ..Default::default()
                };
                let demote = DemoteVoter {
                    to_store: 2,
                    peer_id: 12,
                    is_witness: w,
                };
                for v2 in [false, true] {
                    for (actual, change) in [
                        (
                            remove.get_cmd(&r, v2),
                            ChangePeer {
                                peer: present.then_some(p.clone()),
                                change_type: ChangeType::RemoveNode,
                            },
                        ),
                        (
                            demote.get_cmd(&r, v2),
                            ChangePeer {
                                peer: Some(peer(1, 12, w)),
                                change_type: ChangeType::AddLearnerNode,
                            },
                        ),
                    ] {
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
                        assert_eq!(actual, expected);
                    }
                }
                assert_eq!(remove.to_string(), "remove peer on store 2");
                assert_eq!(
                    demote.to_string(),
                    format!(
                        "demote {}voter peer 12 on store 2 to learner",
                        if w { "witness " } else { "" }
                    )
                );
            }
        }
    }
    for c in fixture()["timeouts"].as_array().unwrap() {
        let size = c[0].as_i64().unwrap();
        let seconds = c[1].as_u64().unwrap();
        assert_eq!(RemovePeer::default().timeout(size).as_secs(), seconds);
        assert_eq!(DemoteVoter::default().timeout(size).as_secs(), seconds);
    }
}
#[test]
fn task_020_costs_and_faults() {
    for c in fixture()["costs"].as_array().unwrap() {
        for light in [false, true] {
            for down in [false, true] {
                for w in [false, true] {
                    for present in [false, true] {
                        let mut r = region(
                            if present {
                                vec![peer(0, 12, w)]
                            } else {
                                vec![]
                            },
                            None,
                        );
                        let size = c[0].as_i64().unwrap();
                        r.stats_mut().approximate_size = size;
                        r.stats_mut().approximate_keys = c[1].as_i64().unwrap();
                        let remove = RemovePeer {
                            from_store: 2,
                            peer_id: 12,
                            is_light_weight: light,
                            is_down_store: down,
                        };
                        let inf = OpInfluence::new();
                        remove.influence(&inf, &r);
                        let witness = w && present;
                        let mut expected = StoreInfluence {
                            region_size: if witness { 0 } else { -size },
                            region_count: -1,
                            witness_count: if witness { -1 } else { 0 },
                            ..Default::default()
                        };
                        if !witness && !light {
                            let cost = c[if down { 3 } else { 2 }].as_i64().unwrap();
                            if cost != 0 {
                                expected.add_step_cost(Type::REMOVE_PEER, cost);
                            }
                        }
                        assert_eq!(
                            inf.get_store_influence(2),
                            expected,
                            "{c} light={light} down={down} witness={w} present={present}"
                        );
                        assert_eq!(inf.snapshot().len(), 1);
                    }
                }
            }
        }
    }
    let cfg = ProviderSnapshot::<String>::default();
    let cluster = BasicCluster::new();
    for leader in [
        None,
        Some(peer(0, 12, false)),
        Some(Peer {
            store_id: 3,
            ..peer(0, 13, false)
        }),
    ] {
        let r = region(vec![peer(0, 12, false)], leader.clone());
        let remove = RemovePeer {
            from_store: 2,
            ..Default::default()
        };
        for health in [false, true] {
            assert_eq!(
                remove.check_in_progress(&cluster, &cfg, &r, health, 0),
                if leader.as_ref().is_some_and(|p| p.store_id == 2) {
                    Err(RemovePeerError::LeaderPeer)
                } else {
                    Ok(())
                }
            );
        }
    }
    assert_eq!(
        RemovePeerError::LeaderPeer.to_string(),
        "cannot remove leader peer"
    );
    let empty = region(vec![], None);
    assert_eq!(
        RemovePeer::default().check_in_progress(&cluster, &cfg, &empty, false, 0),
        Err(RemovePeerError::LeaderPeer)
    );
    // Real successive heartbeat snapshots: joint role is not yet a learner; missing is idempotently removed.
    let demote = DemoteVoter {
        to_store: 2,
        peer_id: 12,
        is_witness: false,
    };
    for (role, finished) in [(0, false), (3, false), (1, true)] {
        let p = peer(role, 12, false);
        let r = region(vec![p.clone()], Some(p));
        assert_eq!(demote.is_finish(&r), finished);
    }
    let remove = RemovePeer {
        from_store: 2,
        ..Default::default()
    };
    assert!(remove.is_finish(&empty));
    assert!(remove.is_finish(&empty));
    let inf = OpInfluence::new();
    inf.update_store(2, |s| {
        s.region_size = i64::MIN;
        s.region_count = i64::MIN;
        s.witness_count = i64::MIN;
    });
    let mut r = region(vec![peer(0, 12, false)], None);
    r.stats_mut().approximate_size = 1;
    remove.influence(&inf, &r);
    assert_eq!(inf.get_store_influence(2).region_size, i64::MAX);
    assert_eq!(inf.get_store_influence(2).region_count, i64::MAX);
    remove.influence(&inf, &region(vec![peer(0, 12, true)], None));
    assert_eq!(inf.get_store_influence(2).witness_count, i64::MAX);
}
