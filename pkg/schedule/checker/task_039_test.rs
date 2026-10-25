// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderContext, OpStep};
use serde_json::{Value, json};
use std::sync::Arc;

fn step_value(s: &OpStep) -> Value {
    match s {
        OpStep::PromoteLearner(p) => json!(["promote", p.to_store, p.peer_id, p.is_witness]),
        OpStep::TransferLeader(p) => json!(["transfer", p.from_store, p.to_store, p.to_stores]),
        OpStep::ChangePeerV2Leave(p) => json!([
            "leave",
            p.promote_learners
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>(),
            p.demote_voters
                .iter()
                .map(|p| json!([p.to_store, p.peer_id, p.is_witness]))
                .collect::<Vec<_>>()
        ]),
        other => panic!("unexpected step {other:?}"),
    }
}

#[tokio::test]
async fn task_039_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_039.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    assert_eq!(fixture["go_execution"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let roles = c["roles"].as_array().unwrap();
        let mut peers: Vec<_> = roles
            .iter()
            .enumerate()
            .map(|(i, r)| Peer {
                id: 101 + i as u64,
                store_id: 1 + i as u64,
                role: match r.as_str().unwrap() {
                    "l" => PeerRole::Learner,
                    "i" => PeerRole::IncomingVoter,
                    "d" => PeerRole::DemotingVoter,
                    _ => PeerRole::Voter,
                },
                is_witness: false,
            })
            .collect();
        if let Some(order) = c["order"].as_array() {
            peers = order
                .iter()
                .map(|s| {
                    peers
                        .iter()
                        .find(|p| p.store_id == s.as_u64().unwrap())
                        .unwrap()
                        .clone()
                })
                .collect();
        }
        let leader = c["leader"].as_u64().unwrap_or(1);
        let pending = peers
            .iter()
            .filter(|p| {
                c["pending"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(p.store_id)))
            })
            .cloned()
            .collect();
        let region = RegionInfo::new(
            RegionMeta {
                id: 39,
                peers: peers.clone(),
                epoch: Some(RegionEpoch {
                    conf_ver: 7,
                    version: 9,
                }),
                ..Default::default()
            },
            peers.iter().find(|p| p.store_id == leader).cloned(),
            vec![],
            pending,
        );
        let stores = StoresInfo::new();
        for id in 1..=3 {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            // Freeze Go's load-based choice: store 2 wins over store 3.
            s.set_leader_count(if id == 3 { 100 } else { 1 });
            stores.put_store(Arc::new(s));
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.enable_placement_rules = c["placement"] == true;
        config.schedule.enable_joint_consensus = c["joint"] == true;
        let learner = LearnerChecker::default();
        let joint = JointStateChecker::default();
        let pause = if c["checker"] == "learner" {
            &learner.pause
        } else {
            &joint.pause
        };
        if let Some(ttl) = c["pause"].as_i64() {
            pause.pause_or_resume_at(ttl, 1000);
        }
        if let Some(ttl) = c["resume"].as_i64() {
            pause.pause_or_resume_at(ttl, 1000);
        }
        let context = BuilderContext {
            stores: &stores,
            config: &config,
            rules: &[],
            now_unix_nanos: (1000 + c["elapsed"].as_i64().unwrap_or(0)) as i128 * 1_000_000_000,
        };
        let out = if c["checker"] == "learner" {
            learner.check(context, &region, true).await
        } else {
            joint.check(context, &region).await
        };
        assert_eq!(
            json!(
                out.errors
                    .iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
            ),
            c["errors"],
            "{name}"
        );
        match out.operator {
            None => assert!(c["steps"].is_null(), "{name}: missing operator"),
            Some(op) => {
                assert!(c["steps"].is_array(), "{name}: unexpected operator");
                assert_eq!(
                    op.desc(),
                    if c["checker"] == "learner" {
                        "promote-learner"
                    } else {
                        "leave-joint-state"
                    },
                    "{name}"
                );
                assert_eq!(op.region_id(), 39);
                assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
                assert_eq!(op.kind().0, c["kind"].as_u64().unwrap() as u32, "{name}");
                assert_eq!(
                    op.priority_level(),
                    if c["checker"] == "learner" {
                        PriorityLevel::MEDIUM
                    } else {
                        PriorityLevel::HIGH
                    },
                    "{name}"
                );
                assert_eq!(
                    json!(
                        (0..op.len())
                            .map(|i| step_value(op.step(i).unwrap()))
                            .collect::<Vec<_>>()
                    ),
                    c["steps"],
                    "{name}"
                );
                assert!(op.start());
                assert!(
                    op.check(&region).is_some(),
                    "{name}: recovery starts unfinished"
                );
                let mut repaired = peers.clone();
                let mut new_leader = leader;
                for step in c["steps"].as_array().unwrap() {
                    match step[0].as_str().unwrap() {
                        "promote" => {
                            repaired
                                .iter_mut()
                                .find(|p| p.store_id == step[1].as_u64().unwrap())
                                .unwrap()
                                .role = PeerRole::Voter
                        }
                        "transfer" => new_leader = step[2].as_u64().unwrap(),
                        "leave" => {
                            for p in &mut repaired {
                                p.role = match p.role {
                                    PeerRole::IncomingVoter => PeerRole::Voter,
                                    PeerRole::DemotingVoter => PeerRole::Learner,
                                    r => r,
                                };
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                let after = RegionInfo::new(
                    RegionMeta {
                        peers: repaired.clone(),
                        ..region.meta().clone()
                    },
                    repaired.iter().find(|p| p.store_id == new_leader).cloned(),
                    vec![],
                    vec![],
                );
                // The frozen expected membership is the next external heartbeat,
                // not an application helper copied from the operator implementation.
                assert!(op.check(&after).is_none(), "{name}: completed heartbeat");
                assert_eq!(
                    op.status(),
                    astersql_pd_schedule_operator::SUCCESS,
                    "{name}"
                );
            }
        }
        assert_eq!(
            region.meta().peers,
            peers,
            "{name}: checker changed original region"
        );
    }
    println!("task_039 fixture: 24 cases passed, 0 failed");
}

#[test]
fn task_039_pause_boundaries_and_concurrent_resume() {
    let pause = Arc::new(PauseController::default());
    assert!(!pause.is_paused_at(1000));
    pause.pause_or_resume_at(10, 1000);
    assert!(pause.is_paused_at(1009));
    assert!(!pause.is_paused_at(1010));
    pause.pause_or_resume_at(2, 1001); // overwrite, do not extend old TTL
    assert!(!pause.is_paused_at(1003));
    pause.pause_or_resume_at(0, 1001);
    assert!(!pause.is_paused_at(1001));
    pause.pause_or_resume_at(-1, 1001);
    assert!(!pause.is_paused_at(1001));
    pause.pause_or_resume_at(1, i64::MAX); // Go int64 wrapping arithmetic
    assert!(!pause.is_paused_at(i64::MAX));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let p = Arc::clone(&pause);
            scope.spawn(move || {
                for _ in 0..1000 {
                    p.pause_or_resume_at(10, 1000);
                    let _ = p.is_paused_at(1000);
                }
            });
        }
    });
    let p = Arc::clone(&pause);
    std::thread::spawn(move || p.pause_or_resume_at(0, 1000))
        .join()
        .unwrap();
    assert!(!pause.is_paused_at(1000));
    pause.pause_or_resume(0);
    assert!(!pause.is_paused());
}
