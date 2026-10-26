// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_schedule_config::ScheduleConfig;
use astersql_pd_schedule_operator::OpInfluence;
use astersql_pd_schedule_plan::BalanceSchedulerPlan;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct Case {
    name: String,
    resource: i32,
    policy: i32,
    ratio: f64,
    max_count: usize,
    average: i64,
    candidate: Option<i64>,
    source: i64,
    target: i64,
    source_influence: i64,
    target_influence: i64,
    weight: f64,
    tolerant: i64,
    delta: i64,
    source_score: f64,
    target_score: f64,
    balance: bool,
}
fn store(id: u64, value: i64, weight: f64) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    });
    s.set_leader_count(value);
    s.set_leader_size(value);
    s.set_region_size(value);
    s.set_witness_count(value);
    s.set_leader_weight(weight);
    s.set_region_weight(weight);
    Arc::new(s)
}
#[test]
fn task_067_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_067.json"
    ))
    .unwrap();
    let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    let n = cases.len();
    for c in cases {
        let kind = ScheduleKind::new(
            ResourceKind::from_i32(c.resource),
            SchedulePolicy::from_i32(c.policy),
        );
        let config = ScheduleConfig {
            tolerant_size_ratio: c.ratio,
            region_score_formula_version: "v2".into(),
            high_space_ratio: 0.7,
            low_space_ratio: 0.8,
            ..Default::default()
        };
        let influence = OpInfluence::new();
        for (id, value) in [(1, c.source_influence), (2, c.target_influence)] {
            influence.update_store(id, |i| {
                i.leader_count = value;
                i.leader_size = value;
                i.region_count = value;
                i.region_size = value;
                i.witness_count = value;
            });
        }
        let region = c.candidate.map(|size| {
            let mut r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
            r.stats_mut().approximate_size = size;
            Arc::new(r)
        });
        let plan = BalanceSchedulerPlan {
            source: Some(store(1, c.source, c.weight)),
            target: Some(store(2, c.target, 1.0)),
            region,
            ..Default::default()
        };
        let solver = BalanceSolver::new(kind, &config, c.average, c.max_count);
        assert_eq!(
            solver.tolerant_resource(),
            c.tolerant,
            "{} tolerance",
            c.name
        );
        assert_eq!(
            solver.region_score_delta(&plan),
            c.delta,
            "{} delta",
            c.name
        );
        let result = solver.evaluate(&plan, &influence).unwrap();
        assert_eq!(result.source_score, c.source_score, "{} source", c.name);
        assert_eq!(result.target_score, c.target_score, "{} target", c.name);
        assert_eq!(result.should_balance(), c.balance, "{} decision", c.name);
        // The same margin applies through v1's high-space branch.
        if c.resource == 1 {
            let mut v1 = config.clone();
            v1.region_score_formula_version = "v1".into();
            let mut p = plan.clone();
            for s in [&mut p.source, &mut p.target] {
                Arc::make_mut(s.as_mut().unwrap()).update_raw_stats(astersql_pd_core::StoreStats {
                    capacity: 1 << 40,
                    available: 1 << 40,
                    ..Default::default()
                });
            }
            let result1 = BalanceSolver::new(kind, &v1, c.average, c.max_count)
                .evaluate(&p, &influence)
                .unwrap();
            assert_eq!(result1, result, "{} v1", c.name);
        }
    }
    println!("task_067: {n} frozen cases passed (plus region v1 replays)");
}
#[test]
fn task_067_missing_resources_and_nan() {
    let cfg = ScheduleConfig::default();
    let solver = BalanceSolver::new(
        ScheduleKind::new(ResourceKind::LEADER, SchedulePolicy::BY_COUNT),
        &cfg,
        0,
        0,
    );
    let influence = OpInfluence::new();
    let mut p = BalanceSchedulerPlan::default();
    assert_eq!(
        solver.evaluate(&p, &influence),
        Err(BalanceError::MissingSource)
    );
    p.source = Some(store(1, 20, 1.0));
    assert_eq!(
        solver.evaluate(&p, &influence),
        Err(BalanceError::MissingTarget)
    );
    p.target = Some(store(2, 0, 1.0));
    p.source = Some(store(1, 20, f64::NAN));
    assert!(!solver.evaluate(&p, &influence).unwrap().should_balance());
    assert_eq!(influence.get_store_influence(1).leader_count, 0);
}

#[test]
fn task_067_resource_selection_and_wrapping() {
    let config = ScheduleConfig {
        tolerant_size_ratio: 1.0,
        region_score_formula_version: "v2".into(),
        ..Default::default()
    };
    let influence = OpInfluence::new();
    influence.update_store(1, |i| {
        i.leader_count = 2;
        i.leader_size = 3;
        i.region_count = 4;
        i.region_size = 5;
        i.witness_count = 6;
    });
    let plan = BalanceSchedulerPlan {
        source: Some(store(1, 100, 1.0)),
        target: Some(store(2, 0, 1.0)),
        ..Default::default()
    };
    for (resource, policy, expected_source, expected_target) in [
        (ResourceKind::LEADER, SchedulePolicy::BY_COUNT, 97.0, 1.0),
        (ResourceKind::LEADER, SchedulePolicy::BY_SIZE, 87.0, 10.0),
        (ResourceKind::REGION, SchedulePolicy::BY_COUNT, 70.0, 10.0),
        (ResourceKind::REGION, SchedulePolicy::BY_SIZE, 65.0, 10.0),
        (ResourceKind::WITNESS, SchedulePolicy::BY_COUNT, 93.0, 1.0),
    ] {
        let result = BalanceSolver::new(ScheduleKind::new(resource, policy), &config, 10, 0)
            .evaluate(&plan, &influence)
            .unwrap();
        assert_eq!(result.source_score, expected_source);
        assert_eq!(result.target_score, expected_target);
    }
    // Go signed multiplication and negation wrap even in debug Rust builds.
    influence.update_store(1, |i| i.region_size = i64::MIN);
    let result = BalanceSolver::new(
        ScheduleKind::new(ResourceKind::REGION, SchedulePolicy::BY_SIZE),
        &config,
        10,
        0,
    )
    .evaluate(&plan, &influence)
    .unwrap();
    assert_eq!(result.source_score, 100.0); // v2 rejects the negative projected size
    assert_eq!(influence.get_store_influence(1).region_size, i64::MIN);
    let unknown = BalanceSolver::new(
        ScheduleKind::new(ResourceKind::from_i32(99), SchedulePolicy::BY_COUNT),
        &config,
        10,
        0,
    )
    .evaluate(&plan, &influence)
    .unwrap();
    assert_eq!(unknown.source_score, 0.0);
    assert!(!unknown.should_balance());
}
