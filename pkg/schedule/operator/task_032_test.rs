// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::SplitPolicy;
use serde_json::{Value, json};
use std::sync::Arc;
fn peer(p: &Value) -> Peer {
    Peer {
        store_id: p[0].as_u64().unwrap(),
        role: match p[1].as_str().unwrap() {
            "l" => PeerRole::Learner,
            "i" => PeerRole::IncomingVoter,
            "d" => PeerRole::DemotingVoter,
            _ => PeerRole::Voter,
        },
        id: p[2].as_u64().unwrap(),
        is_witness: p[3].as_bool().unwrap(),
    }
}
fn region(peers: Vec<Peer>, id: u64, reverse: bool, no_leader: bool) -> RegionInfo {
    let leader = if no_leader {
        None
    } else {
        Some(peers[0].clone())
    };
    RegionInfo::new(
        RegionMeta {
            id,
            peers,
            key_range: astersql_pd_core::KeyRange {
                start_key: if reverse { b"m".to_vec() } else { vec![0, 128] },
                end_key: if reverse { vec![] } else { b"m".to_vec() },
            },
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        leader,
        vec![],
        vec![],
    )
}
fn value(s: &OpStep) -> Value {
    match s {
        OpStep::SplitRegion(s) => json!(["split", s.policy.as_raw(), s.split_keys]),
        OpStep::MergeRegion(s) => json!(["merge", s.is_passive]),
        OpStep::TransferLeader(s) => {
            assert!(s.to_stores.is_empty());
            json!(["transfer", s.from_store, s.to_store])
        }
        OpStep::AddLearner(s) => {
            assert_eq!(s.send_store, 1);
            json!([
                "add",
                s.to_store,
                s.peer_id,
                s.is_witness,
                s.is_light_weight
            ])
        }
        OpStep::RemovePeer(s) => {
            assert!(!s.is_down_store);
            json!(["remove", s.from_store, s.peer_id, s.is_light_weight])
        }
        OpStep::BecomeWitness(s) => json!(["witness", s.store_id, s.peer_id]),
        OpStep::BecomeNonWitness(s) => json!(["nonwitness", s.store_id, s.peer_id, s.send_store]),
        OpStep::BatchSwitchWitness(s) => json!([
            "batch",
            s.to_witnesses
                .iter()
                .map(|p| json!([p.store_id, p.peer_id]))
                .collect::<Vec<_>>(),
            s.to_non_witnesses
                .iter()
                .map(|p| json!([p.store_id, p.peer_id, p.send_store]))
                .collect::<Vec<_>>()
        ]),
        OpStep::ChangePeerV2Enter(s) => json!([
            "enter",
            s.promote_learners
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>(),
            s.demote_voters
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>()
        ]),
        OpStep::ChangePeerV2Leave(s) => json!([
            "leave",
            s.promote_learners
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>(),
            s.demote_voters
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>()
        ]),
        s => panic!("unexpected {s:?}"),
    }
}
#[tokio::test]
async fn task_032_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_032.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    let mut success = 0;
    let mut errors = 0;
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let origin = c["origin"]
            .as_array()
            .unwrap()
            .iter()
            .map(peer)
            .collect::<Vec<_>>();
        let r = region(
            origin.clone(),
            32,
            c["reverse"] == true,
            c["no_leader"] == true,
        );
        let stores = StoresInfo::new();
        for id in 1..=5 {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            s.set_last_heartbeat_unix_nanos(
                if c["blocked"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(id)))
                {
                    0
                } else {
                    1_000_000_000_000
                },
            );
            if c["load"] == true {
                s.set_leader_count(if id == 2 { 100 } else { 10 });
            }
            stores.put_store(Arc::new(s));
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.schedule.enable_joint_consensus = c["joint"] == true;
        config.replication.enable_placement_rules = c["placement"] == true;
        let factory = RangeOperatorFactory::new(
            "factory-32",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 1_000_000_000_000,
            },
            &r,
            true,
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
        let target = c["target"]
            .as_array()
            .map(|a| a.iter().map(peer).collect::<Vec<_>>())
            .unwrap_or_else(|| origin.clone());
        let t = region(target.clone(), 33, c["reverse"] != true, false);
        let result = match c["op"].as_str().unwrap() {
            "split" => create_split_region_operator(
                "factory-32",
                &r,
                OpKind::SPLIT,
                SplitPolicy::from_raw(c["policy"].as_i64().unwrap() as i32),
                if c["keys"].is_array() {
                    vec![]
                } else {
                    vec![vec![0, 255], vec![0, 255], vec![]]
                },
            )
            .map(|p| vec![p]),
            "merge" => factory
                .create_merge(&t, OpKind::MERGE, allocate)
                .await
                .map(Vec::from),
            "scatter" => factory
                .create_scatter(
                    target.iter().map(|p| (p.store_id, p.clone())).collect(),
                    c["leader"].as_u64().unwrap_or(0),
                    c["index"].as_u64().unwrap_or(0) as usize,
                    c["skip"] == true,
                    c["admin"] == true,
                    allocate,
                )
                .await
                .map(|p| vec![p]),
            "leave" => factory.create_leave_joint().await.map(|p| vec![p]),
            "witness" => factory
                .create_witness(c["store"].as_u64().unwrap(), c["witness"] == true, allocate)
                .await
                .map(|p| vec![p]),
            "move_witness" => factory
                .create_move_witness(
                    c["store"].as_u64().unwrap(),
                    c["target_store"].as_u64().unwrap(),
                    allocate,
                )
                .await
                .map(|p| vec![p]),
            _ => unreachable!(),
        };
        assert_eq!(
            allocated,
            c["allocated"].as_u64().unwrap_or(0),
            "{name} allocations"
        );
        assert_eq!(
            result.as_ref().err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name} error"
        );
        assert_eq!(r.meta().peers, origin, "{name} input ownership");
        let ops = match result {
            Ok(o) => {
                success += 1;
                o
            }
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        let op = &ops[0];
        assert_eq!(op.desc(), "factory-32");
        assert_eq!(op.region_id(), 32);
        assert_eq!(op.region_epoch(), r.meta().epoch.as_ref());
        assert_eq!(
            op.kind().0,
            c["kind"].as_u64().unwrap() as u32,
            "{name} kind"
        );
        assert_eq!(
            Value::Array((0..op.len()).map(|i| value(op.step(i).unwrap())).collect()),
            c["steps"],
            "{name} steps"
        );
        if c["op"] == "merge" {
            let passive = &ops[1];
            assert!(passive.start());
            assert_eq!(passive.check(&t), passive.step(0));
            let OpStep::MergeRegion(passive_step) = passive.step(0).unwrap() else {
                panic!()
            };
            assert!(passive_step.get_cmd(&t, true).is_none());
            let mut merged = t.meta().clone();
            merged.key_range.start_key = vec![42];
            let merged = RegionInfo::new(merged, t.leader().cloned(), vec![], vec![]);
            assert!(passive.check(&merged).is_none());
            assert_eq!(passive.status(), SUCCESS);
            assert_eq!(passive.region_id(), 33);
            assert_eq!(passive.kind(), op.kind());
            assert_eq!(passive.timeout_nanos(), op.timeout_nanos());
            assert_eq!(
                passive.additional_info().get(RELATED_MERGE_REGION),
                Some("32".into())
            );
            assert_eq!(
                op.additional_info().get(RELATED_MERGE_REGION),
                Some("33".into())
            );
            assert_eq!(
                passive.step(0),
                Some(&OpStep::MergeRegion(MergeRegion {
                    from_region: Some(r.meta().clone()),
                    to_region: Some(t.meta().clone()),
                    is_passive: true
                }))
            );
            let OpStep::MergeRegion(m) = op.step(op.len() - 1).unwrap() else {
                panic!()
            };
            assert_eq!(m.from_region.as_ref(), Some(r.meta()));
            assert_eq!(m.to_region.as_ref(), Some(t.meta()));
            assert!(m.get_cmd(&r, true).unwrap().merge.is_some());
        }
        if let Some(OpStep::SplitRegion(s)) = op.step(0) {
            assert_eq!(s.start_key, r.meta().key_range.start_key);
            assert_eq!(s.end_key, r.meta().key_range.end_key);
            assert_eq!(
                op.additional_info().get("region-start-key"),
                Some(if c["reverse"] == true { "6D" } else { "0080" }.into())
            );
            assert!(s.get_cmd(&r, true).split_region.is_some());
        }
        replay(name, op, &r, c);
        for i in 0..op.len() {
            let s = op.step(i).unwrap();
            s.influence(&OpInfluence::new(), &r);
            assert!(!s.timeout(1).is_zero());
        }
    }
    println!("task032: {success} successful cases, {errors} expected errors");
}
#[test]
fn task_032_membership_matching() {
    let p = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let a = region(vec![p.clone()], 1, false, false);
    let b = region(
        vec![Peer {
            id: 99,
            ..p.clone()
        }],
        2,
        false,
        false,
    );
    assert!(is_region_match(&a, &b));
    for changed in [
        Peer {
            role: PeerRole::Learner,
            ..p.clone()
        },
        Peer {
            is_witness: true,
            ..p.clone()
        },
        Peer { store_id: 2, ..p },
    ] {
        assert!(!is_region_match(
            &a,
            &region(vec![changed], 2, false, false)
        ));
    }
    assert!(!is_region_match(
        &a,
        &region(
            vec![
                b.meta().peers[0].clone(),
                Peer {
                    store_id: 3,
                    ..Default::default()
                }
            ],
            2,
            false,
            false
        )
    ));
}

fn replay(name: &str, op: &Operator, origin: &RegionInfo, case: &Value) {
    let mut peers = origin.meta().peers.clone();
    let mut leader = origin.leader().map_or(0, |p| p.store_id);
    let mut meta = origin.meta().clone();
    assert!(op.start());
    for i in 0..op.len() {
        let step = op.step(i).unwrap();
        meta.peers = peers.clone();
        let before = RegionInfo::new(
            meta.clone(),
            peers.iter().find(|p| p.store_id == leader).cloned(),
            vec![],
            vec![],
        );
        assert_eq!(op.check(&before), Some(step), "{name} pending {i}");
        let mut conf = 0;
        match step {
            OpStep::SplitRegion(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                meta.key_range.end_key = vec![5];
            }
            OpStep::MergeRegion(s) => {
                assert!(!s.get_cmd(&before, true).unwrap().is_empty());
                assert!(!step.is_finish(&before));
                continue;
            }
            OpStep::TransferLeader(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                leader = s.to_store;
            }
            OpStep::AddLearner(s) => {
                assert!(!s.get_cmd(&before, true).unwrap().is_empty());
                peers.push(Peer {
                    id: s.peer_id,
                    store_id: s.to_store,
                    role: PeerRole::Learner,
                    is_witness: s.is_witness,
                });
                conf = 1;
            }
            OpStep::RemovePeer(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                assert_ne!(s.from_store, leader);
                peers.retain(|p| p.store_id != s.from_store);
                conf = 1;
            }
            OpStep::BecomeWitness(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                peers
                    .iter_mut()
                    .find(|p| p.store_id == s.store_id)
                    .unwrap()
                    .is_witness = true;
                conf = 1;
            }
            OpStep::BecomeNonWitness(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                peers
                    .iter_mut()
                    .find(|p| p.store_id == s.store_id)
                    .unwrap()
                    .is_witness = false;
                conf = 1;
            }
            OpStep::BatchSwitchWitness(s) => {
                assert!(!s.get_cmd(&before, true).is_empty());
                for p in &mut peers {
                    if s.to_witnesses.iter().any(|n| n.store_id == p.store_id) {
                        p.is_witness = true;
                        conf += 1;
                    }
                    if s.to_non_witnesses.iter().any(|n| n.store_id == p.store_id) {
                        p.is_witness = false;
                        conf += 1;
                    }
                }
            }
            OpStep::ChangePeerV2Enter(s) => {
                assert!(!s.get_cmd(&before, true).unwrap().is_empty());
                conf = (s.promote_learners.len() + s.demote_voters.len()) as u64;
                for p in &mut peers {
                    if s.promote_learners.iter().any(|n| n.to_store == p.store_id) {
                        p.role = if conf == 1 {
                            PeerRole::Voter
                        } else {
                            PeerRole::IncomingVoter
                        };
                    }
                    if s.demote_voters.iter().any(|n| n.to_store == p.store_id) {
                        p.role = if conf == 1 {
                            PeerRole::Learner
                        } else {
                            PeerRole::DemotingVoter
                        };
                    }
                }
            }
            OpStep::ChangePeerV2Leave(s) => {
                assert!(!s.get_cmd(&before, true).unwrap().is_empty());
                conf = (s.promote_learners.len() + s.demote_voters.len()) as u64;
                for p in &mut peers {
                    p.role = match p.role {
                        PeerRole::IncomingVoter => PeerRole::Voter,
                        PeerRole::DemotingVoter => PeerRole::Learner,
                        r => r,
                    };
                }
            }
            _ => unreachable!(),
        }
        meta.peers = peers.clone();
        let after = RegionInfo::new(
            meta.clone(),
            peers.iter().find(|p| p.store_id == leader).cloned(),
            vec![],
            vec![],
        );
        assert!(step.is_finish(&after), "{name} ack {i}");
        assert_eq!(step.conf_ver_changed(&after), conf, "{name} confver {i}");
    }
    meta.peers = peers.clone();
    let after = RegionInfo::new(
        meta,
        peers.iter().find(|p| p.store_id == leader).cloned(),
        vec![],
        vec![],
    );
    if case["op"] == "merge" {
        assert!(op.check(&after).is_some());
    } else {
        assert!(op.check(&after).is_none());
        assert_eq!(op.status(), SUCCESS);
    }
    let mut final_peers = case["final_peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(peer)
        .collect::<Vec<_>>();
    final_peers.sort_by_key(|p| p.store_id);
    peers.sort_by_key(|p| p.store_id);
    assert_eq!(peers, final_peers, "{name} final peers");
    assert_eq!(
        leader,
        case["final_leader"].as_u64().unwrap(),
        "{name} final leader"
    );
}
