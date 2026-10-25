// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_schedule_config::ProviderSnapshot;
use serde_json::{Value, json};
use std::sync::Arc;
fn peers(v: &Value) -> Vec<Peer> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| Peer {
            store_id: p[0].as_u64().unwrap(),
            id: p[2].as_u64().unwrap(),
            role: if p[1] == "l" {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: p[3].as_bool().unwrap(),
        })
        .collect()
}
#[tokio::test]
async fn task_030_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_030.json"
    ))
    .unwrap();
    assert_eq!(f["cases"].as_array().unwrap().len(), 21);
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        let now = 1_000_000_000_000i128;
        for s in 1..=4 {
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
                id: 30,
                peers: origin.clone(),
                ..Default::default()
            },
            Some(origin[0].clone()),
            vec![],
            vec![],
        );
        let mut b = Builder::new(
            "witness",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: now,
            },
            &region,
            BuilderOptions::default(),
        );
        b.set_peers(
            peers(&c["target"])
                .into_iter()
                .map(|p| (p.store_id, p))
                .collect(),
        );
        if let Some(m) = c["mutation"].as_array() {
            if m[0] == "w" {
                b.become_witness(m[1].as_u64().unwrap());
            } else {
                b.become_non_witness(m[1].as_u64().unwrap());
            }
        }
        if let Some(s) = c["leader"].as_u64() {
            b.set_leader(s);
        }
        let mut allocated = 0;
        let mut allocate = || {
            allocated += 1;
            if c["allocation_fault"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(99 + allocated)
            }
        };
        let result = if c["joint"] == true {
            b.build_joint(OpKind::ADMIN, SimpleBuildOptions::default(), &mut allocate)
                .await
        } else {
            b.build_simple(OpKind::ADMIN, SimpleBuildOptions::default(), &mut allocate)
                .await
        };
        assert_eq!(allocated, c["allocated"].as_u64().unwrap_or(0), "{name}");
        assert_eq!(
            result.as_ref().err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name}"
        );
        let op = match result {
            Ok(op) => op,
            Err(e) => {
                assert_eq!(b.error(), Some(&e));
                continue;
            }
        };
        let mut actual = vec![];
        for i in 0..op.len() {
            actual.push(match op.step(i).unwrap() {
                OpStep::BecomeWitness(s) => json!(["w", s.store_id, s.peer_id]),
                OpStep::BecomeNonWitness(s) => {
                    assert_eq!(s.send_store, 1);
                    json!(["n", s.store_id, s.peer_id])
                }
                OpStep::BatchSwitchWitness(s) => {
                    for n in &s.to_non_witnesses {
                        assert_eq!(n.send_store, 1);
                    }
                    json!([
                        "batch",
                        s.to_witnesses
                            .iter()
                            .map(|s| s.store_id)
                            .collect::<Vec<_>>(),
                        s.to_non_witnesses
                            .iter()
                            .map(|s| s.store_id)
                            .collect::<Vec<_>>()
                    ])
                }
                OpStep::AddLearner(s) => {
                    assert_eq!(s.send_store, 1);
                    json!(["add", s.to_store, s.peer_id, s.is_witness])
                }
                OpStep::PromoteLearner(s) => {
                    json!(["promote", s.to_store, s.peer_id, s.is_witness])
                }
                OpStep::RemovePeer(s) => json!(["remove", s.from_store, s.peer_id]),
                OpStep::TransferLeader(s) => json!(["transfer", s.from_store, s.to_store]),
                OpStep::ChangePeerV2Enter(s) => json!([
                    "enter",
                    s.promote_learners
                        .iter()
                        .map(|s| json!([s.to_store, s.is_witness]))
                        .collect::<Vec<_>>(),
                    s.demote_voters
                        .iter()
                        .map(|s| json!([s.to_store, s.is_witness]))
                        .collect::<Vec<_>>()
                ]),
                OpStep::ChangePeerV2Leave(s) => json!([
                    "leave",
                    s.promote_learners
                        .iter()
                        .map(|s| json!([s.to_store, s.is_witness]))
                        .collect::<Vec<_>>(),
                    s.demote_voters
                        .iter()
                        .map(|s| json!([s.to_store, s.is_witness]))
                        .collect::<Vec<_>>()
                ]),
                s => panic!("{name}: unexpected {s:?}"),
            });
        }
        assert_eq!(Value::Array(actual), c["steps"], "{name}");
        let mut state = origin.clone();
        let mut leader = origin[0].store_id;
        for i in 0..op.len() {
            let step = op.step(i).unwrap();
            let mut switches = vec![];
            match step {
                OpStep::BecomeWitness(s) => switches.push((s.store_id, s.peer_id, true)),
                OpStep::BecomeNonWitness(s) => switches.push((s.store_id, s.peer_id, false)),
                OpStep::BatchSwitchWitness(s) => {
                    switches.extend(s.to_witnesses.iter().map(|s| (s.store_id, s.peer_id, true)));
                    switches.extend(
                        s.to_non_witnesses
                            .iter()
                            .map(|s| (s.store_id, s.peer_id, false)),
                    );
                }
                OpStep::AddLearner(s) => state.push(Peer {
                    id: s.peer_id,
                    store_id: s.to_store,
                    role: PeerRole::Learner,
                    is_witness: s.is_witness,
                }),
                OpStep::RemovePeer(s) => {
                    assert_ne!(leader, s.from_store);
                    state.retain(|p| p.store_id != s.from_store);
                }
                OpStep::PromoteLearner(s) => {
                    let p = state.iter_mut().find(|p| p.store_id == s.to_store).unwrap();
                    assert_eq!(p.id, s.peer_id);
                    assert_eq!(p.is_witness, s.is_witness);
                    p.role = PeerRole::Voter;
                }
                OpStep::TransferLeader(s) => {
                    assert_eq!(leader, s.from_store);
                    leader = s.to_store;
                    let p = state.iter().find(|p| p.store_id == leader).unwrap();
                    assert!(!p.is_learner() && !p.is_witness);
                }
                OpStep::ChangePeerV2Enter(s) => {
                    let single = s.promote_learners.len() + s.demote_voters.len() == 1;
                    for n in &s.promote_learners {
                        let p = state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                        assert_eq!(p.is_witness, n.is_witness);
                        p.role = if single {
                            PeerRole::Voter
                        } else {
                            PeerRole::IncomingVoter
                        };
                    }
                    for n in &s.demote_voters {
                        assert_ne!(leader, n.to_store);
                        let p = state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                        assert_eq!(p.is_witness, n.is_witness);
                        p.role = if single {
                            PeerRole::Learner
                        } else {
                            PeerRole::DemotingVoter
                        };
                    }
                }
                OpStep::ChangePeerV2Leave(_) => {
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
            if !switches.is_empty() {
                let observed = RegionInfo::new(
                    RegionMeta {
                        id: 30,
                        peers: state.clone(),
                        ..Default::default()
                    },
                    state.iter().find(|p| p.store_id == leader).cloned(),
                    vec![],
                    vec![],
                );
                let command = match step {
                    OpStep::BecomeWitness(s) => s.get_cmd(&observed, true),
                    OpStep::BecomeNonWitness(s) => s.get_cmd(&observed, true),
                    OpStep::BatchSwitchWitness(s) => s.get_cmd(&observed, true),
                    _ => unreachable!(),
                };
                let cmd = command.switch_witnesses.unwrap();
                assert_eq!(
                    cmd.switch_witnesses
                        .iter()
                        .map(|s| {
                            let s = s.as_ref().unwrap();
                            (s.peer_id, s.is_witness)
                        })
                        .collect::<Vec<_>>(),
                    switches.iter().map(|s| (s.1, s.2)).collect::<Vec<_>>()
                );
                for &(store, id, w) in &switches {
                    let p = state.iter_mut().find(|p| p.store_id == store).unwrap();
                    assert_eq!(p.id, id);
                    if w {
                        assert_ne!(store, leader);
                    }
                    p.is_witness = w;
                }
            }
            let observed = RegionInfo::new(
                RegionMeta {
                    id: 30,
                    peers: state.clone(),
                    ..Default::default()
                },
                state.iter().find(|p| p.store_id == leader).cloned(),
                vec![],
                vec![],
            );
            assert!(step.is_finish(&observed), "{name} finish {i}");
            let conf = match step {
                OpStep::TransferLeader(_) => 0,
                OpStep::ChangePeerV2Enter(s) => {
                    (s.promote_learners.len() + s.demote_voters.len()) as u64
                }
                OpStep::ChangePeerV2Leave(s) => {
                    (s.promote_learners.len() + s.demote_voters.len()) as u64
                }
                _ => switches.len().max(1) as u64,
            };
            assert_eq!(step.conf_ver_changed(&observed), conf, "{name} confver {i}");
            if switches.iter().any(|s| !s.2) {
                let pending: Vec<_> = state
                    .iter()
                    .filter(|p| switches.iter().any(|s| s.0 == p.store_id && !s.2))
                    .cloned()
                    .collect();
                let waiting = RegionInfo::new(
                    observed.meta().clone(),
                    observed.leader().cloned(),
                    vec![],
                    pending,
                );
                assert!(!step.is_finish(&waiting), "{name} must wait for snapshot");
            }
        }
        let mut expected: Vec<_> = b.target_peers().values().cloned().collect();
        for p in &mut expected {
            if let Some(o) = origin.iter().find(|o| o.store_id == p.store_id) {
                p.id = o.id;
            } else if p.id == 0 {
                p.id = 100;
            }
        }
        state.sort_by_key(|p| p.store_id);
        expected.sort_by_key(|p| p.store_id);
        assert_eq!(state, expected, "{name} final peers");
        assert_eq!(region.meta().peers, origin);
    }
}
