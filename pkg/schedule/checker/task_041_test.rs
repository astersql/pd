// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, NodeState, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
    StoreStats, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpStep, SUCCESS};
use serde_json::{Value, json};
use std::sync::Arc;

fn step_value(step: &OpStep) -> Value {
    match step {
        OpStep::AddLearner(s) => json!(["add", s.to_store, s.peer_id]),
        OpStep::PromoteLearner(s) => json!(["promote", s.to_store, s.peer_id]),
        OpStep::RemovePeer(s) => json!(["remove", s.from_store, s.peer_id]),
        OpStep::TransferLeader(s) => json!(["transfer", s.from_store, s.to_store]),
        s => panic!("unexpected repair step {s:?}"),
    }
}
#[tokio::test]
async fn task_041_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_041.json"
    ))
    .unwrap();
    assert_eq!(fixture["go_execution"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let contains = |key: &str, id: u64| c[key].as_array().unwrap().contains(&json!(id));
        let peers: Vec<_> = c["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                let id = id.as_u64().unwrap();
                Peer {
                    id: 100 + id,
                    store_id: id,
                    role: if contains("learners", id) {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    ..Default::default()
                }
            })
            .collect();
        let region = RegionInfo::new(
            RegionMeta {
                id: 41,
                peers: peers.clone(),
                epoch: Some(RegionEpoch {
                    conf_ver: 7,
                    version: 9,
                }),
                ..Default::default()
            },
            peers
                .iter()
                .find(|p| json!(p.store_id) == c["leader"])
                .cloned(),
            peers
                .iter()
                .filter(|p| contains("down", p.store_id))
                .map(|p| DownPeer {
                    peer: p.clone(),
                    down_seconds: c["down_seconds"].as_u64().unwrap_or(0),
                })
                .collect(),
            vec![],
        );
        let stores = StoresInfo::new();
        const NOW: i128 = 10_000_000_000_000;
        for id in 1..=4 {
            if contains("missing", id) || (id == 4 && c["target"] == "absent") {
                continue;
            }
            let mut s = StoreInfo::new(StoreMeta {
                id,
                node_state: if contains("offline", id) {
                    NodeState::Removing
                } else if id == 4 && c["target"] == "removed" {
                    NodeState::Removed
                } else {
                    NodeState::Serving
                },
                ..Default::default()
            });
            s.set_last_heartbeat_unix_nanos(
                (NOW - if contains("down", id) {
                    c["age_ns"].as_i64().unwrap() as i128
                } else {
                    0
                }) as i64,
            );
            s.set_leader_count(if id == 1 { 1 } else { 100 });
            s.update_raw_stats(StoreStats {
                capacity: 1_000_000_000_000,
                available: 900_000_000_000,
                used_size: 100_000_000_000,
                is_busy: id == 4 && c["target"] == "busy",
                ..Default::default()
            });
            stores.put_store(Arc::new(s));
        }
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.max_replicas = c["max"].as_u64().unwrap();
        conf.schedule.enable_remove_down_replica = c["remove_down"].as_bool().unwrap();
        conf.schedule.enable_replace_offline_replica = c["replace_offline"].as_bool().unwrap();
        conf.schedule.max_store_down_time = "30m".parse().unwrap();
        conf.schedule.region_score_formula_version = "v2".into();
        conf.schedule.high_space_ratio = 0.7;
        conf.schedule.low_space_ratio = 0.8;
        conf.schedule.max_snapshot_count = 64;
        let checker = ReplicaFaultChecker::default();
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(10, 10000);
        }
        let out = checker
            .check(
                ReplicaFaultContext {
                    config: &conf,
                    stores: &stores,
                    rules: &[],
                    now_unix_nanos: NOW,
                    supports_conf_change_v2: false,
                },
                &region,
                || {
                    if c["allocation_error"] == true {
                        Err(BuilderError::Allocation(
                            "injected allocator failure".into(),
                        ))
                    } else {
                        Ok(1000)
                    }
                },
            )
            .await;
        assert_eq!(
            out.pending,
            c["pending"].as_bool().unwrap(),
            "{name}: pending"
        );
        assert_eq!(
            out.errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>(),
            c["error"]
                .as_str()
                .map(|e| vec![e.to_owned()])
                .unwrap_or_default(),
            "{name}: errors"
        );
        if let Some(op) = out.operator {
            assert_eq!(json!(op.desc()), c["desc"], "{name}");
            assert_eq!(op.priority_level(), PriorityLevel::HIGH, "{name}");
            assert_eq!(
                op.kind().0 as u64,
                c["kind"].as_u64().unwrap(),
                "{name}: kind"
            );
            assert_eq!(op.region_id(), 41);
            assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
            assert_eq!(
                json!(
                    (0..op.len())
                        .map(|i| step_value(op.step(i).unwrap()))
                        .collect::<Vec<_>>()
                ),
                c["steps"],
                "{name}: steps"
            );
            assert!(op.start());
            let mut heartbeat = region.clone();
            let mut members = peers.clone();
            let mut leader = c["leader"].as_u64().unwrap();
            for expected in c["steps"].as_array().unwrap() {
                assert_eq!(
                    step_value(op.check(&heartbeat).unwrap()),
                    *expected,
                    "{name}: next command"
                );
                let id = expected[1].as_u64().unwrap();
                match expected[0].as_str().unwrap() {
                    "add" => members.push(Peer {
                        id: 1000,
                        store_id: id,
                        role: PeerRole::Learner,
                        ..Default::default()
                    }),
                    "promote" => {
                        members.iter_mut().find(|p| p.store_id == id).unwrap().role =
                            PeerRole::Voter
                    }
                    "remove" => members.retain(|p| p.store_id != id),
                    "transfer" => leader = expected[2].as_u64().unwrap(),
                    _ => unreachable!(),
                }
                heartbeat = RegionInfo::new(
                    RegionMeta {
                        peers: members.clone(),
                        ..region.meta().clone()
                    },
                    members.iter().find(|p| p.store_id == leader).cloned(),
                    vec![],
                    vec![],
                );
            }
            assert!(op.check(&heartbeat).is_none(), "{name}: completed");
            assert_eq!(op.status(), SUCCESS);
        } else {
            assert!(c["desc"].is_null(), "{name}: missing operator");
        }
        assert_eq!(region.meta().peers, peers, "{name}: immutable snapshot");
    }
    println!(
        "task_041: 32 fixture cases passed, 0 failed; every generated operation applied heartbeat by heartbeat"
    );
}
