// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::PeerRoleType;
use serde_json::{Value, json};
use std::sync::Arc;
fn peer(p: &Value) -> Peer {
    Peer {
        store_id: p[0].as_u64().unwrap(),
        id: p[2].as_u64().unwrap(),
        role: match p[1].as_str().unwrap() {
            "l" => PeerRole::Learner,
            "i" => PeerRole::IncomingVoter,
            "d" => PeerRole::DemotingVoter,
            _ => PeerRole::Voter,
        },
        is_witness: p[3].as_bool().unwrap(),
    }
}
fn snapshot(state: &[Peer], leader: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: 31,
            peers: state.to_vec(),
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        state.iter().find(|p| p.store_id == leader).cloned(),
        vec![],
        vec![],
    )
}
fn changes(p: &[PromoteLearner]) -> Vec<Value> {
    p.iter()
        .map(|s| json!([s.to_store, s.peer_id, s.is_witness]))
        .collect()
}
fn demotions(p: &[DemoteVoter]) -> Vec<Value> {
    p.iter()
        .map(|s| json!([s.to_store, s.peer_id, s.is_witness]))
        .collect()
}
fn step_value(step: &OpStep) -> Value {
    match step {
        OpStep::AddLearner(s) => {
            assert_eq!(s.send_store, 1);
            assert!(!s.is_light_weight);
            json!(["add", s.to_store, s.peer_id, s.is_witness])
        }
        OpStep::PromoteLearner(s) => json!(["promote", s.to_store, s.peer_id, s.is_witness]),
        OpStep::RemovePeer(s) => {
            assert!(!s.is_light_weight && !s.is_down_store);
            json!(["remove", s.from_store, s.peer_id])
        }
        OpStep::TransferLeader(s) => json!(["transfer", s.from_store, s.to_store, s.to_stores]),
        OpStep::ChangePeerV2Enter(s) => json!([
            "enter",
            changes(&s.promote_learners),
            demotions(&s.demote_voters)
        ]),
        OpStep::ChangePeerV2Leave(s) => json!([
            "leave",
            changes(&s.promote_learners),
            demotions(&s.demote_voters)
        ]),
        s => panic!("unexpected factory step {s:?}"),
    }
}
#[tokio::test]
async fn task_031_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_031.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    let (mut success, mut errors) = (0, 0);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        for s in 1..=5 {
            if c["missing_store"] == s {
                continue;
            }
            let mut st = StoreInfo::new(StoreMeta {
                id: s,
                ..Default::default()
            });
            st.set_last_heartbeat_unix_nanos(if c["blocked_store"] == s {
                0
            } else {
                1_000_000_000_000
            });
            stores.put_store(Arc::new(st));
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.enable_joint_consensus = c["joint"] == true;
        config.replication.enable_placement_rules = c["placement"] == true;
        let origin: Vec<_> = c["origin"].as_array().unwrap().iter().map(peer).collect();
        let region = RegionInfo::new(
            snapshot(&origin, 1).meta().clone(),
            Some(origin[0].clone()),
            vec![],
            c["pending"]
                .as_u64()
                .map(|s| vec![origin.iter().find(|p| p.store_id == s).unwrap().clone()])
                .unwrap_or_default(),
        );
        let factory = PeerOperatorFactory::new(
            "factory-31",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 1_000_000_000_000,
            },
            &region,
            c["support"] == true,
        );
        let mut allocated = 0;
        let allocate = || {
            allocated += 1;
            if c["fault"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(100 + allocated)
            }
        };
        let p = if c["peer"].is_null() {
            None
        } else {
            Some(peer(&c["peer"]))
        };
        let store = c["store"].as_u64().unwrap();
        let old = c["old"].as_u64().unwrap();
        let leader = c["leader"].as_u64().unwrap();
        let result = match c["op"].as_str().unwrap() {
            "add" => factory.create_add_peer(p, OpKind::ADMIN, allocate).await,
            "remove" => {
                factory
                    .create_remove_peer(store, OpKind::ADMIN, allocate)
                    .await
            }
            "promote" => factory.create_promote_learner(store, allocate).await,
            "demote" => factory.create_demote_voter(store, allocate).await,
            "promote_remove" => {
                factory
                    .create_promote_learner_and_remove_peer(store, old, allocate)
                    .await
            }
            "demote_remove" => {
                factory
                    .create_demote_learner_and_remove_peer(store, old, allocate)
                    .await
            }
            "transfer" => {
                factory
                    .create_transfer_leader(
                        leader,
                        c["alternatives"]
                            .as_array()
                            .map(|v| v.iter().map(|s| s.as_u64().unwrap()).collect())
                            .unwrap_or_default(),
                        OpKind::ADMIN,
                        allocate,
                    )
                    .await
            }
            "force" => {
                factory
                    .create_force_transfer_leader(leader, OpKind::ADMIN, allocate)
                    .await
            }
            "move" => {
                factory
                    .create_move_peer(old, p, OpKind::ADMIN, allocate)
                    .await
            }
            "move_leader" => {
                factory
                    .create_move_leader(old, p, OpKind::ADMIN, allocate)
                    .await
            }
            "replace" => {
                factory
                    .create_replace_leader_peer(old, p, leader, OpKind::ADMIN, allocate)
                    .await
            }
            "region" => {
                factory
                    .create_move_region(
                        c["roles"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|p| {
                                (
                                    p[0].as_u64().unwrap(),
                                    match p[1].as_str().unwrap() {
                                        "leader" => PeerRoleType::Leader,
                                        "follower" => PeerRoleType::Follower,
                                        "learner" => PeerRoleType::Learner,
                                        _ => PeerRoleType::Voter,
                                    },
                                )
                            })
                            .collect(),
                        OpKind::ADMIN,
                        allocate,
                    )
                    .await
            }
            _ => unreachable!(),
        };
        assert_eq!(
            allocated,
            c["allocated"].as_u64().unwrap(),
            "{name} allocation"
        );
        assert_eq!(
            result.as_ref().err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name} error"
        );
        assert_eq!(region.meta().peers, origin, "{name} input ownership");
        let op = match result {
            Ok(op) => {
                success += 1;
                op
            }
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        assert_eq!(op.desc(), "factory-31");
        assert_eq!(op.region_id(), 31);
        assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
        assert_eq!(
            op.kind().0,
            u32::try_from(c["kind"].as_u64().unwrap()).unwrap(),
            "{name} kind"
        );
        assert_eq!(
            Value::Array(
                (0..op.len())
                    .map(|i| step_value(op.step(i).unwrap()))
                    .collect()
            ),
            c["steps"],
            "{name} steps"
        );
        assert!(op.start());
        let mut state = origin.clone();
        let mut leader = 1;
        for i in 0..op.len() {
            let step = op.step(i).unwrap();
            let before = snapshot(&state, leader);
            assert_eq!(op.check(&before), Some(step), "{name} pending {i}");
            let influence = OpInfluence::new();
            step.influence(&influence, &before);
            assert!(!step.timeout(10).is_zero());
            match step {
                OpStep::AddLearner(s) => {
                    assert!(s.get_cmd(&before, true).is_some());
                    assert!(state.iter().all(|p| p.store_id != s.to_store));
                    state.push(Peer {
                        id: s.peer_id,
                        store_id: s.to_store,
                        role: PeerRole::Learner,
                        is_witness: s.is_witness,
                    });
                }
                OpStep::PromoteLearner(s) => {
                    let cmd = s.get_cmd(&before, true);
                    assert!(cmd.change_peer_v2.is_some());
                    let p = state.iter_mut().find(|p| p.store_id == s.to_store).unwrap();
                    assert_eq!(p.id, s.peer_id);
                    assert!(p.is_learner());
                    p.role = PeerRole::Voter;
                }
                OpStep::RemovePeer(s) => {
                    let cmd = s.get_cmd(&before, true);
                    assert!(cmd.change_peer_v2.is_some());
                    assert_ne!(leader, s.from_store);
                    assert_eq!(
                        state
                            .iter()
                            .find(|p| p.store_id == s.from_store)
                            .unwrap()
                            .id,
                        s.peer_id
                    );
                    state.retain(|p| p.store_id != s.from_store);
                }
                OpStep::TransferLeader(s) => {
                    let cmd = s.get_cmd(&before, true);
                    assert!(cmd.transfer_leader.is_some());
                    assert_eq!(leader, s.from_store);
                    leader = s.to_store;
                    let p = state.iter().find(|p| p.store_id == leader).unwrap();
                    assert!(!p.is_learner() && !p.is_witness && p.role != PeerRole::DemotingVoter);
                }
                OpStep::ChangePeerV2Enter(s) => {
                    assert!(s.get_cmd(&before, true).unwrap().change_peer_v2.is_some());
                    let single = s.promote_learners.len() + s.demote_voters.len() == 1;
                    for n in &s.promote_learners {
                        let p = state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                        assert_eq!(p.id, n.peer_id);
                        p.role = if single {
                            PeerRole::Voter
                        } else {
                            PeerRole::IncomingVoter
                        };
                    }
                    for n in &s.demote_voters {
                        if single {
                            assert_ne!(leader, n.to_store);
                        }
                        let p = state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                        assert_eq!(p.id, n.peer_id);
                        p.role = if single {
                            PeerRole::Learner
                        } else {
                            PeerRole::DemotingVoter
                        };
                    }
                }
                OpStep::ChangePeerV2Leave(s) => {
                    assert!(s.get_cmd(&before, true).unwrap().change_peer_v2.is_some());
                    for p in &mut state {
                        p.role = match p.role {
                            PeerRole::IncomingVoter => PeerRole::Voter,
                            PeerRole::DemotingVoter => PeerRole::Learner,
                            r => r,
                        };
                    }
                }
                _ => unreachable!(),
            }
            let after = snapshot(&state, leader);
            assert!(step.is_finish(&after), "{name} ack {i}");
            let expected_conf = match step {
                OpStep::TransferLeader(_) => 0,
                OpStep::ChangePeerV2Enter(s) => {
                    (s.promote_learners.len() + s.demote_voters.len()) as u64
                }
                OpStep::ChangePeerV2Leave(s) => {
                    (s.promote_learners.len() + s.demote_voters.len()) as u64
                }
                _ => 1,
            };
            assert_eq!(
                step.conf_ver_changed(&after),
                expected_conf,
                "{name} confver {i}"
            );
        }
        state.sort_by_key(|p| p.store_id);
        assert_eq!(
            state,
            c["final_peers"]
                .as_array()
                .unwrap()
                .iter()
                .map(peer)
                .collect::<Vec<_>>(),
            "{name} final peers"
        );
        assert_eq!(
            leader,
            c["final_leader"].as_u64().unwrap(),
            "{name} final leader"
        );
        assert!(op.check(&snapshot(&state, leader)).is_none());
        assert_eq!(op.status(), SUCCESS);
    }
    println!(
        "task031: {} cases, {success} success, {errors} expected errors; full steps, kind, identity, heartbeat replay",
        cases.len()
    );
}
