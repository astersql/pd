// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::PeerRoleType;
use serde_json::{Value, json};
use std::sync::Arc;
fn peers(v: &Value) -> Vec<Peer> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| Peer {
            store_id: p[0].as_u64().unwrap(),
            id: p[2].as_u64().unwrap(),
            role: match p[1].as_str().unwrap() {
                "l" => PeerRole::Learner,
                "i" => PeerRole::IncomingVoter,
                _ => PeerRole::Voter,
            },
            ..Default::default()
        })
        .collect()
}
#[tokio::test]
async fn task_029_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_029.json"
    ))
    .unwrap();
    assert_eq!(f["cases"].as_array().unwrap().len(), 21);
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        let now = 1_000_000_000_000i128;
        for s in 1..=5 {
            if c["missing"]
                .as_array()
                .is_some_and(|v| v.contains(&s.into()))
            {
                continue;
            }
            let mut st = StoreInfo::new(StoreMeta {
                id: s,
                ..Default::default()
            });
            st.set_last_heartbeat_unix_nanos(now as i64);
            stores.put_store(Arc::new(st));
        }
        let config = ProviderSnapshot::<()>::default();
        let origin = peers(&c["origin"]);
        let region = RegionInfo::new(
            RegionMeta {
                id: 29,
                peers: origin.clone(),
                ..Default::default()
            },
            Some(origin[0].clone()),
            vec![],
            vec![],
        );
        let mut b = Builder::new(
            "joint",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: now,
            },
            &region,
            BuilderOptions::default(),
        );
        let mut target = peers(&c["target"]);
        if let Some(s) = c["witness_target"].as_u64() {
            target
                .iter_mut()
                .find(|p| p.store_id == s)
                .unwrap()
                .is_witness = true;
        }
        b.set_peers(target.into_iter().map(|p| (p.store_id, p)).collect());
        if let Some(s) = c["demote"].as_u64() {
            b.demote_voter(s);
        }
        if let Some(s) = c["leader"].as_u64() {
            b.set_leader(s);
        }
        if let Some(roles) = c["roles"].as_object() {
            b.set_expected_roles(
                roles
                    .iter()
                    .map(|(s, r)| {
                        (
                            s.parse().unwrap(),
                            match r.as_str().unwrap() {
                                "learner" => PeerRoleType::Learner,
                                "follower" => PeerRoleType::Follower,
                                _ => PeerRoleType::Voter,
                            },
                        )
                    })
                    .collect(),
            );
        }
        let mut allocations = 0;
        let result = b
            .build_joint(
                OpKind::ADMIN,
                SimpleBuildOptions {
                    add_light_peer: c["light"] == true,
                    remove_light_peer: c["light"] == true,
                },
                || {
                    allocations += 1;
                    if c["allocation_fault"] == true {
                        Err(BuilderError::Allocation("injected".into()))
                    } else {
                        Ok(99 + allocations)
                    }
                },
            )
            .await;
        assert_eq!(allocations, c["allocated"].as_u64().unwrap_or(0), "{name}");
        assert_eq!(
            result.as_ref().err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name}"
        );
        if let Err(e) = result {
            assert_eq!(b.error(), Some(&e));
            continue;
        }
        let op = result.unwrap();
        let mut actual = vec![];
        let mut region_kind = false;
        let mut leader_kind = false;
        let mut state = origin.clone();
        let mut leader = 1;
        for i in 0..op.len() {
            let step = op.step(i).unwrap();
            let encoded = match step {
                OpStep::AddLearner(s) => {
                    region_kind = true;
                    assert_eq!(s.send_store, 1);
                    assert_eq!(s.is_light_weight, c["light"] == true);
                    state.push(Peer {
                        id: s.peer_id,
                        store_id: s.to_store,
                        role: PeerRole::Learner,
                        is_witness: s.is_witness,
                    });
                    json!(["add", s.to_store, s.peer_id])
                }
                OpStep::RemovePeer(s) => {
                    region_kind = true;
                    assert_eq!(s.is_light_weight, c["light"] == true);
                    assert!(!s.is_down_store);
                    state.retain(|p| p.store_id != s.from_store);
                    json!(["remove", s.from_store, s.peer_id])
                }
                OpStep::PromoteLearner(s) => {
                    let p = state.iter_mut().find(|p| p.store_id == s.to_store).unwrap();
                    assert_eq!(p.id, s.peer_id);
                    p.role = PeerRole::Voter;
                    json!(["promote", s.to_store, s.peer_id])
                }
                OpStep::TransferLeader(s) => {
                    leader_kind = true;
                    assert_eq!(s.from_store, leader);
                    assert!(s.to_stores.is_empty());
                    leader = s.to_store;
                    let p = state.iter().find(|p| p.store_id == leader).unwrap();
                    assert!(matches!(p.role, PeerRole::Voter | PeerRole::IncomingVoter));
                    json!(["transfer", s.from_store, s.to_store])
                }
                OpStep::ChangePeerV2Enter(s) => {
                    let single = s.promote_learners.len() + s.demote_voters.len() == 1;
                    for p in &s.promote_learners {
                        let n = state.iter_mut().find(|n| n.store_id == p.to_store).unwrap();
                        assert_eq!(n.id, p.peer_id);
                        n.role = if single {
                            PeerRole::Voter
                        } else {
                            PeerRole::IncomingVoter
                        };
                    }
                    for p in &s.demote_voters {
                        let n = state.iter_mut().find(|n| n.store_id == p.to_store).unwrap();
                        assert_eq!(n.id, p.peer_id);
                        n.role = if single {
                            PeerRole::Learner
                        } else {
                            PeerRole::DemotingVoter
                        };
                    }
                    json!([
                        "enter",
                        s.promote_learners
                            .iter()
                            .map(|p| p.to_store)
                            .collect::<Vec<_>>(),
                        s.demote_voters
                            .iter()
                            .map(|p| p.to_store)
                            .collect::<Vec<_>>()
                    ])
                }
                OpStep::ChangePeerV2Leave(s) => {
                    assert!(
                        !state
                            .iter()
                            .any(|p| p.store_id == leader && p.role == PeerRole::DemotingVoter)
                    );
                    for p in &mut state {
                        p.role = match p.role {
                            PeerRole::IncomingVoter => PeerRole::Voter,
                            PeerRole::DemotingVoter => PeerRole::Learner,
                            r => r,
                        };
                    }
                    json!([
                        "leave",
                        s.promote_learners
                            .iter()
                            .map(|p| p.to_store)
                            .collect::<Vec<_>>(),
                        s.demote_voters
                            .iter()
                            .map(|p| p.to_store)
                            .collect::<Vec<_>>()
                    ])
                }
                s => panic!("unexpected {s:?}"),
            };
            let observed = RegionInfo::new(
                RegionMeta {
                    id: 29,
                    peers: state.clone(),
                    ..Default::default()
                },
                state.iter().find(|p| p.store_id == leader).cloned(),
                vec![],
                vec![],
            );
            assert!(step.is_finish(&observed), "{name} step {i}");
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
                step.conf_ver_changed(&observed),
                expected_conf,
                "{name} confver {i}"
            );
            actual.push(encoded);
        }
        assert_eq!(Value::Array(actual), c["steps"], "{name}");
        assert!(
            !state
                .iter()
                .any(|p| matches!(p.role, PeerRole::IncomingVoter | PeerRole::DemotingVoter)),
            "{name} must leave joint"
        );
        let mut expected = peers(&c["target"]);
        for p in &mut expected {
            if c["demote"] == p.store_id {
                p.role = PeerRole::Learner;
            }
            if let Some(o) = origin.iter().find(|o| o.store_id == p.store_id) {
                p.id = o.id;
            }
            if p.id == 0 {
                p.id = 100;
            }
        }
        state.sort_by_key(|p| p.store_id);
        expected.sort_by_key(|p| p.store_id);
        assert_eq!(state, expected, "{name}");
        assert_eq!(
            op.kind().0,
            OpKind::ADMIN.0
                | if region_kind { OpKind::REGION.0 } else { 0 }
                | if leader_kind { OpKind::LEADER.0 } else { 0 }
        );
        assert_eq!(op.region_id(), 29);
        assert_eq!(region.meta().peers, origin);
    }
}
