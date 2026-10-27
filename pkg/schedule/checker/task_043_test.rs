// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
    StoreStats, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpStep, SUCCESS};
use astersql_pd_schedule_placement::{Rule, RuleManager, fit_region};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::{Value, json};
use std::sync::Arc;
const NOW: i128 = 10_000_000_000_000;
fn step(s: &OpStep) -> Value {
    match s {
        OpStep::AddLearner(s) => json!(["add", s.to_store, s.peer_id, s.is_witness]),
        OpStep::PromoteLearner(s) => json!(["promote", s.to_store, s.peer_id, s.is_witness]),
        OpStep::RemovePeer(s) => json!(["remove", s.from_store, s.peer_id, false]),
        s => panic!("unexpected {s:?}"),
    }
}
fn region() -> RegionInfo {
    let peers = vec![
        Peer {
            id: 101,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 102,
            store_id: 2,
            ..Default::default()
        },
    ];
    RegionInfo::new(
        RegionMeta {
            id: 41,
            peers: peers.clone(),
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    )
}
fn stores(count: u64, engine: &str, busy: bool) -> StoresInfo {
    let stores = StoresInfo::new();
    for id in 1..=count {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            node_state: astersql_pd_core::NodeState::Serving,
            labels: {
                let mut labels = vec![StoreLabel {
                    key: "zone".into(),
                    value: "a".into(),
                }];
                if id == 3 && !engine.is_empty() {
                    labels.push(StoreLabel {
                        key: "engine".into(),
                        value: engine.into(),
                    });
                }
                labels
            },
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(NOW as i64);
        s.set_region_size(id as i64);
        s.set_leader_count(id as i64);
        s.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            is_busy: id == 3 && busy,
            ..Default::default()
        });
        stores.put_store(Arc::new(s));
    }
    stores
}
fn conf() -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.replication.enable_placement_rules = true;
    c.replication.enable_placement_rules_cache = true;
    c.schedule.max_store_down_time = "30m".parse().unwrap();
    c.schedule.max_snapshot_count = 64;
    c
}
fn manager() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(43),
    )
}
#[tokio::test]
async fn task_043_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_043.json"
    ))
    .unwrap();
    assert_eq!(fixture["go_execution"], false);
    for c in fixture["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut region = region();
        if c["no_leader"] == true {
            region = RegionInfo::new(region.meta().clone(), None, vec![], vec![]);
        }
        let stores = stores(
            c["stores"].as_u64().unwrap(),
            c["engine"].as_str().unwrap(),
            c["temporary"] == true,
        );
        let conf = conf();
        let mut rule = Rule {
            group_id: "pd".into(),
            id: "default".into(),
            count: c["count"].as_i64().unwrap(),
            role: c["role"].as_str().unwrap().to_owned().into(),
            is_witness: c["witness"] == true,
            ..Default::default()
        };
        for key in ["engine", "zone"] {
            if let Some(value) = c[key].as_str().filter(|v| !v.is_empty()) {
                rule.label_constraints
                    .push(astersql_pd_schedule_placement::LabelConstraint {
                        key: key.into(),
                        op: astersql_pd_schedule_placement::LabelConstraintOp::In,
                        values: vec![value.into()],
                    });
            }
        }
        if c["isolation"] != "" {
            rule.location_labels = vec!["zone".into()];
            rule.isolation_level = "zone".into();
        }
        let rules = if c["entry"] == "empty" {
            vec![]
        } else {
            vec![Arc::new(rule)]
        };
        let mut m = manager();
        m.initialize(3, &[], "", false, &conf, &[]).await.unwrap();
        let checker = RuleMissingChecker::default();
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(10, 10000);
        }
        let ctx = RuleMissingContext {
            replica: ReplicaFaultContext {
                config: &conf,
                stores: &stores,
                rules: &rules,
                now_unix_nanos: NOW,
                supports_conf_change_v2: false,
            },
            witness_enabled: c["witness_enabled"] == true,
        };
        let mut allocate = || {
            if c["allocation_error"] == true {
                Err(BuilderError::Allocation("injected".into()))
            } else {
                Ok(1000)
            }
        };
        let fit = Arc::new(fit_region(
            &stores.get_stores(),
            &region,
            &rules,
            c["witness_enabled"] == true,
        ));
        let out = match c["entry"].as_str().unwrap() {
            "check" => checker.check(&ctx, &mut m, &region, &mut allocate).await,
            "tiflash" => {
                checker
                    .fix_missing_tiflash_learner_peer(&ctx, &region, &fit, &mut allocate)
                    .await
            }
            _ => {
                checker
                    .check_with_fit(
                        &ctx,
                        &mut m,
                        &region,
                        if c["entry"] == "none" {
                            None
                        } else {
                            Some(fit)
                        },
                        &mut allocate,
                    )
                    .await
            }
        };
        assert_eq!(out.pending, c["pending"] == true, "{name}: pending");
        assert_eq!(
            out.pending_processed,
            c["temporary"] == true,
            "{name}: temporary"
        );
        let errors: Vec<_> = out
            .errors
            .iter()
            .map(|e| match e {
                RuleMissingError::Builder(BuilderError::Allocation(_)) => "allocation".to_string(),
                e => format!("{e:?}"),
            })
            .collect();
        assert_eq!(
            errors,
            c["error"]
                .as_str()
                .map(|e| vec![e.to_string()])
                .unwrap_or_default(),
            "{name}: errors"
        );
        if let Some(op) = out.operator {
            assert_eq!(json!(op.desc()), c["desc"], "{name}");
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            assert_eq!(op.kind().0, 144);
            assert_eq!(op.region_id(), 41);
            assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
            assert_eq!(
                json!(
                    (0..op.len())
                        .map(|i| step(op.step(i).unwrap()))
                        .collect::<Vec<_>>()
                ),
                c["steps"],
                "{name}: steps"
            );
            assert!(op.start());
            let mut hb = region.clone();
            let mut peers = region.meta().peers.clone();
            for expected in c["steps"].as_array().unwrap() {
                assert_eq!(step(op.check(&hb).unwrap()), *expected);
                let id = expected[1].as_u64().unwrap();
                if expected[0] == "add" {
                    peers.push(Peer {
                        id: 1000,
                        store_id: id,
                        role: PeerRole::Learner,
                        is_witness: expected[3] == true,
                    });
                } else {
                    peers.iter_mut().find(|p| p.store_id == id).unwrap().role = PeerRole::Voter;
                }
                hb = RegionInfo::new(
                    RegionMeta {
                        peers: peers.clone(),
                        ..region.meta().clone()
                    },
                    region.leader().cloned(),
                    vec![],
                    vec![],
                );
            }
            assert!(op.check(&hb).is_none());
            assert_eq!(op.status(), SUCCESS);
        } else {
            assert!(c["desc"].is_null(), "{name}: missing operator");
        }
        assert_eq!(region.meta().peers.len(), 2);
        println!("case passed: {name}");
    }
}
#[tokio::test]
async fn task_043_cache_invalidation() {
    let conf = conf();
    let stores = stores(3, "", false);
    let region = region();
    let checker = RuleMissingChecker::default();
    let mut m = manager();
    m.initialize(2, &[], "", false, &conf, &[]).await.unwrap();
    let rules = vec![];
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
    let mut allocate = || Ok(1000);
    for _ in 0..12 {
        assert!(
            checker
                .check(&ctx, &mut m, &region, &mut allocate)
                .await
                .operator
                .is_none()
        );
    }
    assert!(m.is_region_fit_cached(&stores.get_stores(), &region, NOW));
    // Promote the actual canonical cache and prove a hit, then feed that fit into this stage.
    let fit = m.fit_region_without_cache(&stores.get_stores(), &region, &conf);
    for _ in 0..11 {
        m.set_region_fit_cache(&stores.get_stores(), &region, fit.clone(), NOW);
    }
    let cached = m.fit_region(&stores.get_stores(), &region, &conf, NOW);
    assert!(Arc::ptr_eq(
        &cached,
        &m.fit_region(&stores.get_stores(), &region, &conf, NOW)
    ));
    assert!(
        checker
            .check_with_fit(&ctx, &mut m, &region, Some(cached), &mut allocate)
            .await
            .operator
            .is_none()
    );
    let mut updated = m.get_rule("pd", "default").unwrap();
    updated.count = 3;
    m.set_rule(updated, 2, &conf, &[]).await.unwrap();
    assert!(!m.is_region_fit_cached(&stores.get_stores(), &region, NOW));
    let out = checker.check(&ctx, &mut m, &region, &mut allocate).await;
    assert_eq!(out.operator.unwrap().desc(), "add-rule-peer");
    assert!(!m.is_region_fit_cached(&stores.get_stores(), &region, NOW));
}
fn constraint(key: &str, value: &str) -> astersql_pd_schedule_placement::LabelConstraint {
    astersql_pd_schedule_placement::LabelConstraint {
        key: key.into(),
        op: astersql_pd_schedule_placement::LabelConstraintOp::In,
        values: vec![value.into()],
    }
}
#[tokio::test]
async fn task_043_multiple_rules_and_swap() {
    let conf = conf();
    let stores = stores(4, "tiflash", false);
    let region = region();
    let checker = RuleMissingChecker::default();
    let rules = vec![
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "voters".into(),
            count: 2,
            ..Default::default()
        }),
        Arc::new(Rule {
            group_id: "tiflash".into(),
            id: "unavailable".into(),
            role: "learner".to_string().into(),
            count: 1,
            label_constraints: vec![
                constraint("engine", "tiflash"),
                constraint("zone", "absent"),
            ],
            ..Default::default()
        }),
        Arc::new(Rule {
            group_id: "tiflash".into(),
            id: "available".into(),
            role: "learner".to_string().into(),
            count: 1,
            label_constraints: vec![constraint("engine", "tiflash")],
            ..Default::default()
        }),
    ];
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
    let fit = fit_region(&stores.get_stores(), &region, &rules, false);
    let out = checker
        .fix_missing_tiflash_learner_peer(&ctx, &region, &fit, &mut || Ok(1000))
        .await;
    assert!(!out.pending && !out.pending_processed);
    assert!(out.errors.is_empty());
    let op = out.operator.unwrap();
    assert_eq!(
        json!(
            (0..op.len())
                .map(|i| step(op.step(i).unwrap()))
                .collect::<Vec<_>>()
        ),
        json!([["add", 3, 1000, false]])
    );
    // Ordinary Check retains first error diagnostics while returning the next rule's operation.
    let mut m = manager();
    let out = checker
        .check_with_fit(&ctx, &mut m, &region, Some(Arc::new(fit)), &mut || Ok(1000))
        .await;
    assert_eq!(out.errors, vec![RuleMissingError::NoStoreToAdd]);
    assert!(!out.pending);
    assert!(out.operator.is_some());
    // A learner on constrained store2 already belongs to a satisfied broad learner rule.
    // Adding the constrained learner must first move that existing peer to store3.
    let stores = stores_for_swap();
    let mut peers = region.meta().peers.clone();
    peers[1].role = PeerRole::Learner;
    let region = RegionInfo::new(
        RegionMeta {
            peers,
            ..region.meta().clone()
        },
        region.leader().cloned(),
        vec![],
        vec![],
    );
    let rules = vec![
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "voter".into(),
            count: 1,
            ..Default::default()
        }),
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "broad".into(),
            role: "learner".to_string().into(),
            count: 1,
            ..Default::default()
        }),
        Arc::new(Rule {
            group_id: "pd".into(),
            id: "constrained".into(),
            role: "learner".to_string().into(),
            count: 1,
            label_constraints: vec![constraint("zone", "b")],
            ..Default::default()
        }),
    ];
    // Fit each rule in order; explicit fit assignment models the supplied CheckWithFit contract.
    let mut fit = fit_region(&stores.get_stores(), &region, &rules, false);
    // Global fitting may instead reserve store2 for the constrained rule. Both assignments
    // are valid inputs; retain Go's swap scenario using real per-rule fit objects.
    fit.rule_fits = vec![
        astersql_pd_schedule_placement::fit_rule(
            &stores.get_stores(),
            &region,
            rules[0].clone(),
            false,
        ),
        astersql_pd_schedule_placement::fit_rule(
            &stores.get_stores(),
            &region,
            rules[1].clone(),
            false,
        ),
        astersql_pd_schedule_placement::fit_rule(&[], &region, rules[2].clone(), false),
    ];
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
    let out = checker
        .check_with_fit(&ctx, &mut m, &region, Some(Arc::new(fit)), &mut || Ok(1000))
        .await;
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert!(!out.pending);
    let op = out.operator.unwrap();
    assert_eq!(op.desc(), "replace-rule-swap-fit-peer");
    assert_eq!(op.priority_level(), PriorityLevel::HIGH);
    assert_eq!(
        json!(
            (0..op.len())
                .map(|i| step(op.step(i).unwrap()))
                .collect::<Vec<_>>()
        ),
        json!([["add", 3, 1000, false], ["remove", 2, 102, false]])
    );
}
fn stores_for_swap() -> StoresInfo {
    let stores = stores(3, "", false);
    let mut s = (*stores.get_store(2).unwrap()).clone();
    s.set_labels(vec![StoreLabel {
        key: "zone".into(),
        value: "b".into(),
    }]);
    stores.put_store(Arc::new(s));
    stores
}
