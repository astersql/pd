// Copyright 2026 AsterSQL.

use crate::{StoreRateLimit, Type};
use astersql_pd_core_constant::PriorityLevel;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct Scenario {
    name: String,
    rate: f64,
    operations: Vec<Operation>,
    expected: Vec<bool>,
}

#[derive(Deserialize)]
struct Operation {
    action: String,
    #[serde(default)]
    typ: i64,
    #[serde(default)]
    cost: i64,
    #[serde(default)]
    rate: f64,
    #[serde(default)]
    ms: u64,
}

// Each scenario starts at t=0. Only explicit advance operations move Tokio time.
async fn run_scenario(index: usize) {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_04.json")).unwrap();
    let scenario = &scenarios[index];
    let mut limiter = StoreRateLimit::new(scenario.rate);
    let mut actual = Vec::new();
    for op in &scenario.operations {
        let typ = Type::from_i64(op.typ);
        match op.action.as_str() {
            "take" => actual.push(limiter.take(op.cost, typ, PriorityLevel::LOW).await),
            "available" => actual.push(limiter.available(op.cost, typ, PriorityLevel::LOW).await),
            "reset" => {
                limiter.reset(op.rate, typ);
                let expected_rate = if typ == Type::SEND_SNAPSHOT {
                    scenario.rate
                } else {
                    op.rate
                };
                assert_eq!(limiter.rate(typ), expected_rate, "{}", scenario.name);
            }
            "advance" => tokio::time::advance(Duration::from_millis(op.ms)).await,
            other => panic!("unknown action {other}"),
        }
    }
    assert_eq!(actual, scenario.expected, "{}", scenario.name);
}

#[tokio::test(start_paused = true)]
async fn core_task_04_same_rate_preserves_balance() {
    run_scenario(0).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_04_changed_rate_rebuilds_target() {
    run_scenario(1).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_04_zero_initial_and_reset() {
    run_scenario(2).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_04_unlimited_threshold() {
    run_scenario(3).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_04_fractional_refill_and_capacity() {
    run_scenario(4).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_04_snapshot_reset_is_noop() {
    run_scenario(5).await;
}
#[tokio::test(start_paused = true)]
#[should_panic]
async fn core_task_04_invalid_reset_type_panics() {
    StoreRateLimit::new(1.0).reset(2.0, Type::from_i64(4));
}
#[tokio::test(start_paused = true)]
async fn core_task_04_reset_preserves_other_bucket() {
    let mut limiter = StoreRateLimit::new(1.0);
    assert!(
        limiter
            .take(1000, Type::REMOVE_PEER, PriorityLevel::LOW)
            .await
    );
    limiter.reset(2.0, Type::ADD_PEER);
    assert!(
        !limiter
            .available(1, Type::REMOVE_PEER, PriorityLevel::LOW)
            .await
    );
    assert!(limiter.take(2000, Type::ADD_PEER, PriorityLevel::LOW).await);
}
