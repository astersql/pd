// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, NodeState, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta, StoreStats, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpStep};
use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, Rule, fit_region};
use serde_json::{Value, json};
use std::sync::Arc;
const NOW: i128 = 10_000_000_000_000;
fn constraint(op: LabelConstraintOp, id: u64) -> LabelConstraint {
    LabelConstraint {
        key: "slot".into(),
        op,
        values: vec![id.to_string()],
    }
}
fn setup(c: &Value) -> (RegionInfo, StoresInfo, ProviderSnapshot<()>, Vec<Arc<Rule>>) {
    let target = c["target"].as_u64().unwrap_or(3);
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id: 100 + id,
            store_id: id,
            role: if id == target && c["learner"] == true {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: id == 3 && c["witness_peer"] == true,
        })
        .collect();
    let down = peers
        .iter()
        .filter(|p| c["down"] == p.store_id || c["also_down"] == p.store_id)
        .map(|p| DownPeer {
            peer: p.clone(),
            down_seconds: 1,
        })
        .collect();
    let pending = peers
        .iter()
        .filter(|p| c["pending"] == p.store_id)
        .cloned()
        .collect();
    let region = RegionInfo::new(
        RegionMeta {
            id: 44,
            peers: peers.clone(),
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        if c["no_leader"] == true {
            None
        } else {
            Some(peers[0].clone())
        },
        down,
        pending,
    );
    let stores = StoresInfo::new();
    for id in 1..=4 {
        if c["missing_store"] == id || (id == 4 && c["no_target"] == true) {
            continue;
        }
        let mut s = StoreInfo::new(StoreMeta {
            id,
            node_state: if c["offline"] == id {
                NodeState::Removing
            } else {
                NodeState::Serving
            },
            labels: vec![StoreLabel {
                key: "slot".into(),
                value: if id == 4 {
                    target.to_string()
                } else {
                    id.to_string()
                },
            }],
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(
            (NOW - if c["down"] == id {
                c["elapsed"].as_i64().unwrap_or(0) as i128 * 1_000_000_000
            } else {
                0
            }) as i64,
        );
        s.set_region_size(id as i64);
        s.set_leader_count(id as i64);
        s.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            is_busy: c["busy"] == id || (c["block_others"] == true && id != target),
            ..Default::default()
        });
        stores.put_store(Arc::new(s));
    }
    let mut conf = ProviderSnapshot::default();
    conf.replication.enable_placement_rules = true;
    conf.schedule.max_store_down_time = "30m".parse().unwrap();
    conf.schedule.max_snapshot_count = 64;
    conf.schedule.switch_witness_interval = "10m".parse().unwrap();
    let rules = vec![
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "target".into(),
            count: 1,
            role: c["role"].as_str().unwrap_or("voter").to_string().into(),
            is_witness: c["witness_rule"] == true,
            label_constraints: vec![constraint(LabelConstraintOp::In, target)],
            ..Default::default()
        }),
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "rest".into(),
            role: "voter".to_string().into(),
            count: 2,
            label_constraints: vec![constraint(LabelConstraintOp::NotIn, target)],
            ..Default::default()
        }),
    ];
    (region, stores, conf, rules)
}
fn value(s: &OpStep) -> Value {
    match s {
        OpStep::PromoteLearner(s) => json!(["promote", s.to_store, s.peer_id, s.is_witness]),
        OpStep::AddLearner(s) => json!(["add", s.to_store, s.peer_id, s.is_witness]),
        OpStep::RemovePeer(s) => json!(["remove", s.from_store, s.peer_id]),
        OpStep::TransferLeader(s) => {
            assert!(s.to_stores.is_empty());
            json!(["transfer", s.from_store, s.to_store])
        }
        OpStep::BecomeWitness(s) => json!(["witness", s.store_id, s.peer_id]),
        OpStep::BecomeNonWitness(s) => json!(["nonwitness", s.store_id, s.peer_id, s.send_store]),
        s => panic!("unexpected step {s:?}"),
    }
}
#[tokio::test]
async fn task_044_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_044.json"
    ))
    .unwrap();
    assert_eq!(f["go_execution"], false);
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (r, stores, conf, rules) = setup(c);
        let origin = r.meta().clone();
        let ctx = RuleMissingContext {
            replica: ReplicaFaultContext {
                config: &conf,
                stores: &stores,
                rules: &rules,
                now_unix_nanos: NOW,
                supports_conf_change_v2: false,
            },
            witness_enabled: c["enabled"] == true,
        };
        let fit_region_input = if let Some(id) = c["fit_leader"].as_u64() {
            RegionInfo::new(
                r.meta().clone(),
                r.meta().peers.iter().find(|p| p.store_id == id).cloned(),
                r.down_peers().to_vec(),
                r.pending_peers().to_vec(),
            )
        } else {
            r.clone()
        };
        let fit = fit_region(
            &stores.get_stores(),
            &fit_region_input,
            &rules,
            ctx.witness_enabled,
        );
        let mut checker = RuleFaultChecker::default();
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(10, 10000);
        }
        if c["recent"] == true {
            checker.record_region_promote_to_non_witness(
                44,
                NOW,
                conf.schedule.switch_witness_interval.as_nanos() as i128,
            );
        }
        let out = checker
            .check_with_fit(&ctx, &r, &fit, &mut || {
                if c["allocation_error"] == true {
                    Err(BuilderError::Allocation("injected".into()))
                } else {
                    Ok(1000)
                }
            })
            .await;
        assert_eq!(out.pending, c["retry"] == true, "{name}:pending");
        assert_eq!(
            out.pending_processed,
            c["temporary"] == true,
            "{name}:temporary"
        );
        let errors: Vec<_> = out
            .errors
            .iter()
            .map(|e| match e {
                RuleFaultError::Builder(BuilderError::Allocation(_)) => "allocation".to_string(),
                e => format!("{e:?}"),
            })
            .collect();
        assert_eq!(
            json!(errors),
            c["error"].as_str().map(|s| json!([s])).unwrap_or(json!([])),
            "{name}:error"
        );
        if let Some(op) = out.operator {
            assert_eq!(json!(op.desc()), c["desc"], "{name}:desc");
            assert_eq!(
                op.kind().0,
                c["kind"].as_u64().unwrap() as u32,
                "{name}:kind"
            );
            assert_eq!(
                op.priority_level(),
                match c["priority"].as_str().unwrap() {
                    "HIGH" => PriorityLevel::HIGH,
                    "URGENT" => PriorityLevel::URGENT,
                    _ => PriorityLevel::MEDIUM,
                },
                "{name}:priority"
            );
            assert_eq!(op.region_id(), 44);
            assert_eq!(op.region_epoch(), r.meta().epoch.as_ref());
            assert_eq!(
                json!(
                    (0..op.len())
                        .map(|i| value(op.step(i).unwrap()))
                        .collect::<Vec<_>>()
                ),
                c["steps"],
                "{name}:steps"
            );
            apply(&op, &r);
        } else {
            assert!(c["desc"].is_null(), "{name}:missing op");
        }
        assert_eq!(r.meta(), &origin);
        println!("case passed: {name}");
    }
}

fn apply(op: &astersql_pd_schedule_operator::Operator, r: &RegionInfo) {
    assert!(op.start());
    let mut hb = r.clone();
    for i in 0..op.len() {
        let expected = op.step(i).unwrap();
        assert_eq!(value(op.check(&hb).unwrap()), value(expected));
        let mut peers = hb.meta().peers.clone();
        let mut leader = hb.leader().cloned();
        match expected {
            OpStep::AddLearner(s) => peers.push(Peer {
                id: s.peer_id,
                store_id: s.to_store,
                role: PeerRole::Learner,
                is_witness: s.is_witness,
            }),
            OpStep::PromoteLearner(s) => {
                let p = peers.iter_mut().find(|p| p.store_id == s.to_store).unwrap();
                p.role = PeerRole::Voter;
                p.is_witness = s.is_witness;
            }
            OpStep::RemovePeer(s) => peers.retain(|p| p.store_id != s.from_store),
            OpStep::TransferLeader(s) => {
                leader = peers.iter().find(|p| p.store_id == s.to_store).cloned()
            }
            OpStep::BecomeWitness(s) => {
                peers
                    .iter_mut()
                    .find(|p| p.store_id == s.store_id)
                    .unwrap()
                    .is_witness = true
            }
            OpStep::BecomeNonWitness(s) => {
                peers
                    .iter_mut()
                    .find(|p| p.store_id == s.store_id)
                    .unwrap()
                    .is_witness = false
            }
            _ => unreachable!(),
        }
        let mut meta = hb.meta().clone();
        meta.peers = peers;
        meta.epoch.as_mut().unwrap().conf_ver += 1;
        hb = RegionInfo::new(meta, leader, vec![], vec![]);
    }
    assert!(op.check(&hb).is_none());
    assert_eq!(op.status(), astersql_pd_schedule_operator::SUCCESS);
}
#[tokio::test]
async fn task_044_recorder_and_leader_replacement() {
    let (r, stores, conf, rules) = setup(&json!({"target":1,"offline":1}));
    let ctx = RuleMissingContext {
        replica: ReplicaFaultContext {
            config: &conf,
            stores: &stores,
            rules: &rules,
            now_unix_nanos: NOW,
            supports_conf_change_v2: false,
        },
        witness_enabled: false,
    };
    let fit = fit_region(&stores.get_stores(), &r, &rules, false);
    let mut checker = RuleFaultChecker::default();
    for i in 0..13 {
        let op = checker
            .check_with_fit(&ctx, &r, &fit, &mut || Ok(1000))
            .await
            .operator
            .unwrap();
        if i == 0 {
            assert_eq!(op.desc(), "replace-rule-offline-peer");
            assert_eq!(checker.offline_leader_count(1), 1);
        }
        if i == 1 {
            assert_eq!(op.desc(), "replace-rule-offline-leader-peer");
            assert_eq!(value(op.step(0).unwrap()), json!(["transfer", 1, 2]));
        }
        apply(&op, &r);
    }
    assert_eq!(
        (
            checker.offline_leader_count(1),
            checker.offline_leader_count(2),
            checker.offline_leader_count(3)
        ),
        (5, 4, 4)
    );
    let out = checker
        .check_with_fit(&ctx, &r, &fit, &mut || {
            Err(BuilderError::Allocation("injected".into()))
        })
        .await;
    assert!(matches!(
        &out.errors[..],
        [RuleFaultError::Builder(BuilderError::Allocation(_))]
    ));
    assert_eq!(checker.offline_leader_count(2), 4);
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 5,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    checker.refresh(&stores, NOW + 301_000_000_000);
    assert_eq!(checker.offline_leader_count(1), 5);
    stores.delete_store(1);
    checker.refresh(&stores, NOW + 300_000_000_000);
    assert_eq!(checker.offline_leader_count(1), 5);
    checker.refresh(&stores, NOW + 300_000_000_001);
    assert_eq!(checker.offline_leader_count(1), 0);
    assert_eq!(checker.offline_leader_count(2), 0);
}
#[tokio::test]
async fn task_044_cooldown_and_leader_health() {
    let (r, stores, conf, rules) = setup(&json!({"witness_rule":true}));
    let fit = fit_region(&stores.get_stores(), &r, &rules, true);
    let mut checker = RuleFaultChecker::default();
    checker.record_region_promote_to_non_witness(44, NOW, 600_000_000_000);
    for (elapsed, expected) in [(599_999_999_999, false), (600_000_000_000, true)] {
        let now = NOW + elapsed;
        for id in 1..=4 {
            stores.patch_store(id, |s| s.set_last_heartbeat_unix_nanos(now as i64));
        }
        let ctx = RuleMissingContext {
            replica: ReplicaFaultContext {
                config: &conf,
                stores: &stores,
                rules: &rules,
                now_unix_nanos: now,
                supports_conf_change_v2: false,
            },
            witness_enabled: true,
        };
        let op = checker
            .check_with_fit(&ctx, &r, &fit, &mut || Ok(1000))
            .await
            .operator;
        assert_eq!(op.is_some(), expected);
        if let Some(op) = op {
            apply(&op, &r);
        }
        let mut p = r.meta().peers[1].clone();
        assert!(checker.allow_leader(&ctx, &fit, &p).await);
        p.role = PeerRole::Learner;
        assert!(!checker.allow_leader(&ctx, &fit, &p).await);
        p.role = PeerRole::Voter;
        p.is_witness = true;
        assert!(!checker.allow_leader(&ctx, &fit, &p).await);
        p.is_witness = false;
        p.store_id = 999;
        assert!(!checker.allow_leader(&ctx, &fit, &p).await);
        assert!(checker.allow_leader(&ctx, &fit, &r.meta().peers[2]).await);
    }
}
#[tokio::test]
async fn task_044_tiflash_down_and_offline_preflight() {
    // A TiFlash learner never receives the fast-failover witness flag.
    let (r, stores, conf, mut rules) =
        setup(&json!({"learner":true,"role":"learner","down":3,"elapsed":1800}));
    let mut rule = (*rules[0]).clone();
    rule.label_constraints.push(LabelConstraint {
        key: "engine".into(),
        op: LabelConstraintOp::In,
        values: vec!["tiflash".into()],
    });
    rules[0] = Arc::new(rule);
    for id in [3, 4] {
        stores.patch_store(id, |s| {
            let mut labels = s.labels().to_vec();
            labels.push(StoreLabel {
                key: "engine".into(),
                value: "tiflash".into(),
            });
            s.set_labels(labels);
        });
    }
    let ctx = RuleMissingContext {
        replica: ReplicaFaultContext {
            config: &conf,
            stores: &stores,
            rules: &rules,
            now_unix_nanos: NOW,
            supports_conf_change_v2: false,
        },
        witness_enabled: true,
    };
    let fit = fit_region(&stores.get_stores(), &r, &rules, true);
    let mut checker = RuleFaultChecker::default();
    let op = checker
        .check_with_fit(&ctx, &r, &fit, &mut || Ok(1000))
        .await
        .operator
        .unwrap();
    assert_eq!(op.desc(), "replace-rule-down-peer");
    assert_eq!(op.priority_level(), PriorityLevel::HIGH);
    assert_eq!(
        json!(
            (0..op.len())
                .map(|i| value(op.step(i).unwrap()))
                .collect::<Vec<_>>()
        ),
        json!([["add", 4, 1000, false], ["remove", 3, 103]])
    );
    apply(&op, &r);
    // The first offline voter rule defers to an actual missing TiFlash learner.
    let mut meta = r.meta().clone();
    meta.peers.retain(|p| p.store_id != 3);
    let r = RegionInfo::new(meta, r.leader().cloned(), vec![], vec![]);
    stores.patch_store(1, |s| {
        s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false))
    });
    rules.swap(0, 1);
    let ctx = RuleMissingContext {
        replica: ReplicaFaultContext {
            config: &conf,
            stores: &stores,
            rules: &rules,
            now_unix_nanos: NOW,
            supports_conf_change_v2: false,
        },
        witness_enabled: true,
    };
    let fit = fit_region(&stores.get_stores(), &r, &rules, true);
    let out = checker
        .check_with_fit(&ctx, &r, &fit, &mut || Ok(1000))
        .await;
    assert!(out.errors.is_empty());
    assert!(!out.pending && !out.pending_processed);
    let op = out.operator.unwrap();
    assert_eq!(op.desc(), "add-rule-peer");
    assert_eq!(
        json!(
            (0..op.len())
                .map(|i| value(op.step(i).unwrap()))
                .collect::<Vec<_>>()
        ),
        json!([["transfer", 1, 2], ["add", 4, 1000, false]])
    );
    apply(&op, &r);
}
