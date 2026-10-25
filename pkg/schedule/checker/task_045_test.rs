// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, NodeState, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta, StoreStats, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpStep, Operator};
use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, Rule, fit_region};
use serde_json::{Value, json};
use std::sync::Arc;
const NOW: i128 = 10_000_000_000_000;
fn setup(c: &Value) -> (RegionInfo, StoresInfo, ProviderSnapshot<()>, Vec<Arc<Rule>>) {
    let better = c["stage"] == "better";
    let count = if better {
        3
    } else {
        3 + c["orphans"].as_u64().unwrap_or(1)
    };
    let peers: Vec<_> = (1..=count)
        .map(|id| Peer {
            id: 100 + id,
            store_id: id,
            role: if (id > 3 && c["orphan_voter"] != true && c["demote"] != true)
                || (id == 3 && c["demote"] == true)
            {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: c["witness"] == id || (c["witness_rule"] == true && id >= 2),
        })
        .collect();
    let down = peers
        .iter()
        .filter(|p| c["down"] == p.store_id)
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
    let r = RegionInfo::new(
        RegionMeta {
            id: 45,
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
    for id in 1..=count.max(4) {
        if c["missing_store"] == id {
            continue;
        }
        let zone = if id == 3 && c["location"] == true {
            "z3"
        } else if id == 1 {
            "z1"
        } else if id == 4 && c["equal"] != true {
            c["orphan_zone"].as_str().unwrap_or("z3")
        } else {
            "z2"
        };
        let mut labels = vec![
            StoreLabel {
                key: "zone".into(),
                value: zone.into(),
            },
            StoreLabel {
                key: "slot".into(),
                value: id.to_string(),
            },
        ];
        if c["tiflash"] == id {
            labels.push(StoreLabel {
                key: "engine".into(),
                value: "tiflash".into(),
            });
        }
        let mut s = StoreInfo::new(StoreMeta {
            id,
            node_state: NodeState::Serving,
            labels,
            ..Default::default()
        });
        let age = if c["disconnect"] == id || c["also_disconnect"] == id {
            21_000_000_000
        } else if id == 3 {
            c["heartbeat_age"]
                .as_f64()
                .map(|s| (s * 1e9).round() as i128)
                .unwrap_or(0)
        } else {
            0
        };
        s.set_last_heartbeat_unix_nanos((NOW - age) as i64);
        s.set_region_size(id as i64);
        s.set_leader_count(id as i64);
        s.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            is_busy: c["busy"] == id || (c["busy_sources"] == true && (id == 2 || id == 3)),
            ..Default::default()
        });
        stores.put_store(Arc::new(s));
    }
    let mut conf = ProviderSnapshot::default();
    conf.replication.enable_placement_rules = true;
    conf.schedule.region_score_formula_version = "v2".into();
    conf.schedule.max_snapshot_count = 64;
    conf.schedule.enable_joint_consensus = c["disable_joint"] != true;
    let mut rule = Rule {
        group_id: "pd".into(),
        id: "main".into(),
        role: if c["demote"] == true {
            "learner"
        } else {
            "voter"
        }
        .to_string()
        .into(),
        count: if c["demote"] == true {
            1
        } else {
            c["count"].as_i64().unwrap_or(3)
        },
        location_labels: if (better && c["no_labels"] != true) || c["location"] == true {
            vec!["zone".into()]
        } else {
            vec![]
        },
        ..Default::default()
    };
    if c["exclude_leader"] == true {
        rule.label_constraints.push(LabelConstraint {
            key: "slot".into(),
            op: LabelConstraintOp::NotIn,
            values: vec!["1".into()],
        });
    }
    if c["restricted"] == true {
        rule.label_constraints.push(LabelConstraint {
            key: "slot".into(),
            op: LabelConstraintOp::In,
            values: vec!["1".into(), "2".into(), "3".into()],
        });
    }
    let mut rules = vec![];
    if c["demote"] == true || c["witness_rule"] == true {
        let ids = if c["demote"] == true {
            vec!["3".into(), "4".into()]
        } else {
            vec!["2".into(), "3".into(), "4".into()]
        };
        rule.label_constraints = vec![LabelConstraint {
            key: "slot".into(),
            op: LabelConstraintOp::In,
            values: ids.clone(),
        }];
        if c["witness_rule"] == true {
            rule.count = 2;
            rule.is_witness = true;
        }
        rules.push(Arc::new(Rule {
            group_id: "pd".into(),
            id: "rest".into(),
            role: "voter".to_string().into(),
            count: if c["demote"] == true { 2 } else { 1 },
            label_constraints: vec![LabelConstraint {
                key: "slot".into(),
                op: LabelConstraintOp::NotIn,
                values: ids,
            }],
            ..Default::default()
        }));
    }
    rules.push(Arc::new(rule));
    (r, stores, conf, rules)
}
fn value(s: &OpStep) -> Value {
    match s {
        OpStep::AddLearner(s) => json!(["add", s.to_store, s.peer_id, s.is_witness]),
        OpStep::PromoteLearner(s) => json!(["promote", s.to_store, s.peer_id, s.is_witness]),
        OpStep::RemovePeer(s) => json!(["remove", s.from_store, s.peer_id]),
        OpStep::TransferLeader(s) => json!(["transfer", s.from_store, s.to_store]),
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
        s => panic!("unexpected step {s:?}"),
    }
}
fn apply(op: &Operator, r: &RegionInfo) -> RegionInfo {
    assert!(op.start());
    let mut hb = r.clone();
    for i in 0..op.len() {
        let step = op.step(i).unwrap();
        assert_eq!(value(op.check(&hb).unwrap()), value(step));
        let mut meta = hb.meta().clone();
        let mut leader = hb.leader().cloned();
        match step {
            OpStep::AddLearner(s) => meta.peers.push(Peer {
                id: s.peer_id,
                store_id: s.to_store,
                role: PeerRole::Learner,
                is_witness: s.is_witness,
            }),
            OpStep::PromoteLearner(s) => {
                let p = meta
                    .peers
                    .iter_mut()
                    .find(|p| p.store_id == s.to_store)
                    .unwrap();
                p.role = PeerRole::Voter;
                p.is_witness = s.is_witness;
            }
            OpStep::RemovePeer(s) => meta.peers.retain(|p| p.store_id != s.from_store),
            OpStep::TransferLeader(s) => {
                leader = meta
                    .peers
                    .iter()
                    .find(|p| p.store_id == s.to_store)
                    .cloned()
            }
            OpStep::ChangePeerV2Enter(s) => {
                assert_eq!(s.promote_learners.len() + s.demote_voters.len(), 1);
                for p in &s.demote_voters {
                    meta.peers
                        .iter_mut()
                        .find(|n| n.store_id == p.to_store)
                        .unwrap()
                        .role = PeerRole::Learner;
                }
            }
            _ => unreachable!(),
        }
        meta.epoch.as_mut().unwrap().conf_ver += 1;
        hb = RegionInfo::new(meta, leader, vec![], vec![]);
    }
    assert!(op.check(&hb).is_none());
    assert_eq!(op.status(), astersql_pd_schedule_operator::SUCCESS);
    hb
}
#[tokio::test]
async fn task_045_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_045.json"
    ))
    .unwrap();
    assert_eq!(fixture["go_execution"], false);
    for c in fixture["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (r, stores, conf, rules) = setup(c);
        let origin = r.meta().clone();
        let ctx = RuleMissingContext {
            replica: ReplicaFaultContext {
                config: &conf,
                stores: &stores,
                rules: &rules,
                now_unix_nanos: NOW,
                supports_conf_change_v2: c["joint"] == true,
            },
            witness_enabled: true,
        };
        let fit = fit_region(&stores.get_stores(), &r, &rules, true);
        let checker = RuleLocationChecker::default();
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(10, 10000);
        }
        let mut allocation = || {
            if c["allocation_error"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(1000)
            }
        };
        let out = if c["stage"] == "orphan" {
            let result = checker
                .fix_orphan_peers(&ctx, &r, &fit, &mut allocation)
                .await;
            let mut out = RuleMissingOutcome::default();
            match result {
                Ok(op) => out.operator = op,
                Err(e) => out.errors.push(RuleMissingError::Builder(e)),
            };
            out
        } else {
            checker
                .fix_better_location(
                    &ctx,
                    &r,
                    &fit,
                    fit.rule_fits.last().unwrap(),
                    &mut allocation,
                )
                .await
        };
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
                RuleMissingError::Builder(BuilderError::Allocation(_)) => "allocation".into(),
                RuleMissingError::Builder(e) => format!("{e:?}"),
                e => format!("{e:?}"),
            })
            .collect();
        assert_eq!(
            json!(errors),
            c["error"].as_str().map(|s| json!([s])).unwrap_or(json!([])),
            "{name}:errors"
        );
        if let Some(op) = out.operator {
            assert_eq!(op.desc(), c["desc"].as_str().expect(name), "{name}:desc");
            assert_eq!(
                op.priority_level(),
                PriorityLevel::MEDIUM,
                "{name}:priority"
            );
            assert_eq!(
                op.kind().0,
                c["kind"].as_u64().unwrap() as u32,
                "{name}:kind"
            );
            assert_eq!(op.region_id(), 45);
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
            let final_region = apply(&op, &r);
            let after = fit_region(&stores.get_stores(), &final_region, &rules, true);
            for (before, after) in fit.rule_fits.iter().zip(&after.rule_fits) {
                assert!(
                    after.peers.len() >= before.peers.len(),
                    "{name}:rule count degraded"
                );
                assert!(
                    after.peers_with_different_role.len() <= before.peers_with_different_role.len(),
                    "{name}:role degraded"
                );
                assert!(
                    after.isolation_score >= before.isolation_score,
                    "{name}:isolation degraded"
                );
            }
        } else {
            assert!(c["desc"].is_null(), "{name}:missing operator");
        }
        assert_eq!(r.meta(), &origin);
        println!("case passed: {name}");
    }
}

#[tokio::test]
async fn task_045_offline_recorder_refresh_uses_store_ids() {
    // Reuse task44's real fault stage while task45's location stage remains pure.
    let (r, stores, conf, rules) = setup(&json!({"stage":"better"}));
    stores.patch_store(1, |s| {
        s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false))
    });
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
    let mut fault = RuleFaultChecker::default();
    for i in 0..13 {
        let op = fault
            .check_with_fit(&ctx, &r, &fit, &mut || Ok(1000))
            .await
            .operator
            .unwrap();
        if i == 0 {
            assert_eq!(op.desc(), "replace-rule-offline-peer");
        }
        if i == 1 {
            assert_eq!(value(op.step(0).unwrap()), json!(["transfer", 1, 2]));
        }
        apply(&op, &r);
    }
    assert_eq!(
        (
            fault.offline_leader_count(1),
            fault.offline_leader_count(2),
            fault.offline_leader_count(3)
        ),
        (5, 4, 4)
    );
    let out = fault
        .check_with_fit(&ctx, &r, &fit, &mut || {
            Err(BuilderError::Allocation("injected".into()))
        })
        .await;
    assert!(matches!(
        &out.errors[..],
        [RuleFaultError::Builder(BuilderError::Allocation(_))]
    ));
    assert_eq!(fault.offline_leader_count(2), 4);
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 5,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    fault.refresh(&stores, NOW + 301_000_000_000);
    assert_eq!(fault.offline_leader_count(1), 5);
    stores.delete_store(1);
    fault.refresh(&stores, NOW + 300_000_000_000);
    assert_eq!(fault.offline_leader_count(1), 5);
    fault.refresh(&stores, NOW + 300_000_000_001);
    for id in [1, 2, 3] {
        assert_eq!(fault.offline_leader_count(id), 0);
    }
}
