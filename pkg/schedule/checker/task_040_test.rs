// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    NodeState, Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoreStats,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::{EngineFilter, ExcludedFilter, Filter, not_special_engines};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, fit_region};
use serde_json::Value;
use std::sync::Arc;

#[tokio::test]
async fn task_040_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_040.json"
    ))
    .unwrap();
    assert_eq!(fixture["go_execution"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    for c in cases {
        let stores: Vec<_> = c["stores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut s = StoreInfo::new(StoreMeta {
                    id: v[0].as_u64().unwrap(),
                    last_heartbeat: 1_000_000_000,
                    node_state: if v[4] == "removed" {
                        NodeState::Removed
                    } else {
                        NodeState::Serving
                    },
                    ..Default::default()
                });
                s.set_labels(
                    vec![
                        StoreLabel {
                            key: "zone".into(),
                            value: v[1].as_str().unwrap().into(),
                        },
                        StoreLabel {
                            key: "specialUse".into(),
                            value: v[5].as_str().unwrap().into(),
                        },
                        StoreLabel {
                            key: "engine".into(),
                            value: v[6].as_str().unwrap().into(),
                        },
                    ]
                    .into_iter()
                    .filter(|l| !l.value.is_empty())
                    .collect(),
                );
                s.set_region_size(v[2].as_i64().unwrap());
                s.update_raw_stats(StoreStats {
                    capacity: 1_000_000_000_000,
                    available: 900_000_000_000,
                    used_size: 100_000_000_000,
                    is_busy: v[3].as_bool().unwrap(),
                    sending_snap_count: v[7].as_u64().unwrap() as u32,
                    ..Default::default()
                });
                Arc::new(s)
            })
            .collect();
        let peers: Vec<_> = c["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| Peer {
                id: id.as_u64().unwrap() + 100,
                store_id: id.as_u64().unwrap(),
                ..Default::default()
            })
            .collect();
        let region = RegionInfo::new(
            RegionMeta {
                peers,
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        let mut conf = ProviderSnapshot::<()>::default();
        conf.schedule.region_score_formula_version = "v2".into();
        conf.schedule.high_space_ratio = 0.7;
        conf.schedule.low_space_ratio = 0.8;
        conf.schedule.max_snapshot_count = 64;
        let co: Vec<_> = c["co"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                stores
                    .iter()
                    .find(|s| s.id() == id.as_u64().unwrap())
                    .unwrap()
                    .clone()
            })
            .collect();
        let labels: Vec<_> = c["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let engine = EngineFilter::new("rule-checker", not_special_engines());
        let engine_filters: [&dyn Filter<()>; 1] = [&engine];
        let strategy = ReplicaStrategy {
            checker_name: "replica-checker",
            conf: &conf,
            stores: &stores,
            location_labels: &labels,
            isolation_level: c["isolation"].as_str().unwrap(),
            region: &region,
            extra_filters: if c["op"] == "better" {
                &engine_filters
            } else {
                &[]
            },
            fast_failover: c["fast"].as_bool().unwrap(),
            now_unix_nanos: 1_000_000_000,
        };
        let old = c["old"].as_u64().unwrap();
        let op = c["op"].as_str().unwrap();
        let actual = if op == "better" {
            let rules = vec![Arc::new(Rule {
                role: PeerRoleType::Voter,
                count: 3,
                location_labels: labels.clone(),
                ..Default::default()
            })];
            let fit = fit_region(&stores, &region, &rules, false);
            let (old, new, temp) = strategy
                .get_better_location(&region, &fit, &fit.rule_fits[0])
                .await;
            serde_json::json!([old, new, temp])
        } else {
            let result = match op {
                "add" => strategy.select_store_to_add(&co, &[]).await,
                "fix" => strategy.select_store_to_fix(&co, old).await,
                "improve" => strategy.select_store_to_improve(&co, old).await,
                "remove" => (strategy.select_store_to_remove(&co).await, false),
                "remove_temp" => strategy.select_store_to_remove_with_temp_state(&co).await,
                "extra" => {
                    let f = ExcludedFilter::new("test", [], [4]);
                    let filters: [&dyn Filter<()>; 1] = [&f];
                    strategy.select_store_to_add(&co, &filters).await
                }
                _ => panic!("unknown op"),
            };
            serde_json::json!([result.0, result.1])
        };
        assert_eq!(actual, c["expected"], "{}", c["name"]);
    }
    println!(
        "task_040: {} source-derived behavior cases passed",
        cases.len()
    );
}
#[test]
fn task_040_witness_version_gate() {
    for (major, minor, allowed, expected) in [
        (6, 5, true, false),
        (6, 6, true, true),
        (6, 6, false, false),
        (6, 7, true, true),
        (7, 0, true, true),
        (5, 9, true, false),
    ] {
        let mut conf = ProviderSnapshot::default();
        conf.cluster_version = (major, minor);
        conf.schedule.enable_witness = allowed;
        assert_eq!(is_witness_enabled(&conf, |v| *v), expected);
    }
}

#[tokio::test]
async fn task_040_better_location_comparison_boundaries() {
    use astersql_pd_core::PeerRole;
    use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp};
    for mode in ["engine", "role", "constraint"] {
        let stores: Vec<_> = (1..=5)
            .map(|id| {
                let mut s = StoreInfo::new(StoreMeta {
                    id,
                    last_heartbeat: 1_000_000_000,
                    ..Default::default()
                });
                let mut labels = vec![
                    StoreLabel {
                        key: "zone".into(),
                        value: match id {
                            1 | 2 => "a",
                            3 => "b",
                            _ => "c",
                        }
                        .into(),
                    },
                    StoreLabel {
                        key: "class".into(),
                        value: if id == 5 { "other" } else { "main" }.into(),
                    },
                ];
                if mode == "engine" && id == 5 {
                    labels.push(StoreLabel {
                        key: "engine".into(),
                        value: "tiflash".into(),
                    });
                }
                s.set_labels(labels);
                s.set_region_size(id as i64 * 10);
                s.update_raw_stats(StoreStats {
                    capacity: 1_000_000_000_000,
                    available: 900_000_000_000,
                    used_size: 100_000_000_000,
                    ..Default::default()
                });
                Arc::new(s)
            })
            .collect();
        let peers: Vec<_> = [1, 2, 3, 5]
            .into_iter()
            .map(|id| Peer {
                id: 100 + id,
                store_id: id,
                role: if id == 5 && mode == "role" {
                    PeerRole::Learner
                } else {
                    PeerRole::Voter
                },
                is_witness: false,
            })
            .collect();
        let region = RegionInfo::new(
            RegionMeta {
                peers,
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        let class = |value: &str| LabelConstraint {
            key: "class".into(),
            op: LabelConstraintOp::In,
            values: vec![value.into()],
        };
        let main_constraints = if mode == "engine" {
            vec![]
        } else {
            vec![class("main")]
        };
        let other_constraints = if mode == "engine" {
            vec![LabelConstraint {
                key: "engine".into(),
                op: LabelConstraintOp::In,
                values: vec!["tiflash".into()],
            }]
        } else {
            vec![class("other")]
        };
        let rules = vec![
            Arc::new(Rule {
                id: "main".into(),
                role: PeerRoleType::Voter,
                count: 3,
                label_constraints: main_constraints,
                location_labels: vec!["zone".into()],
                ..Default::default()
            }),
            Arc::new(Rule {
                id: "other".into(),
                role: if mode == "role" {
                    PeerRoleType::Learner
                } else {
                    PeerRoleType::Voter
                },
                count: 1,
                label_constraints: other_constraints,
                location_labels: vec!["zone".into()],
                ..Default::default()
            }),
        ];
        let mut fit = fit_region(&stores, &region, &rules, false);
        assert!(fit.is_satisfied(), "{mode}");
        let mut conf = ProviderSnapshot::<()>::default();
        conf.schedule.region_score_formula_version = "v2".into();
        conf.schedule.low_space_ratio = 0.8;
        let labels = vec!["zone".into()];
        let engine = EngineFilter::new("test", not_special_engines());
        let filters: [&dyn Filter<()>; 1] = [&engine];
        let strategy = ReplicaStrategy {
            checker_name: "test",
            conf: &conf,
            stores: &stores,
            location_labels: &labels,
            isolation_level: "",
            region: &region,
            extra_filters: &filters,
            fast_failover: false,
            now_unix_nanos: 1_000_000_000,
        };
        // The unrelated peer on zone c participates only when another same-role
        // rule admits it. Engine and role boundaries leave c as an improvement.
        assert_eq!(
            strategy
                .get_better_location(&region, &fit, &fit.rule_fits[0])
                .await,
            if mode == "constraint" {
                (2, 0, false)
            } else {
                (2, 4, false)
            },
            "{mode}"
        );
        fit.rule_fits[0].peers.push(Peer {
            id: 999,
            store_id: 999,
            ..Default::default()
        });
        let resolved = get_rule_fit_stores(&stores, &fit.rule_fits[0]);
        assert_eq!(
            resolved.iter().map(|s| s.id()).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(Arc::ptr_eq(&resolved[0], &stores[0]));
    }
}
