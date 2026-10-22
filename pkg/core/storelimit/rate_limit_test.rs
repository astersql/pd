// Copyright 2026 AsterSQL.

use crate::{StoreRateLimit, Type, VERSION_V1};
use astersql_pd_core_constant::PriorityLevel;
use serde::Deserialize;

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
    cost: i64,
    typ: i64,
    priority: i32,
}

// All fixture operations run at paused Tokio time t=0; no refill is advanced.
async fn run_scenario(index: usize) {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_03.json")).unwrap();
    let scenario = &scenarios[index];
    let limiter = StoreRateLimit::new(scenario.rate);
    assert_eq!(limiter.version(), VERSION_V1);
    for typ in 0..4 {
        assert_eq!(limiter.rate(Type::from_i64(typ)), scenario.rate);
    }
    let mut actual = Vec::new();
    for op in &scenario.operations {
        let typ = Type::from_i64(op.typ);
        let priority = PriorityLevel::from_i32(op.priority);
        match op.action.as_str() {
            "available" => actual.push(limiter.available(op.cost, typ, priority).await),
            "take" => actual.push(limiter.take(op.cost, typ, priority).await),
            "ack" => limiter.ack(op.cost, typ),
            "feedback" => limiter.feedback(op.cost as f64),
            other => panic!("unknown operation {other}"),
        }
    }
    assert_eq!(actual, scenario.expected, "{}", scenario.name);
}

#[tokio::test(start_paused = true)]
async fn core_task_03_capacity_query_consumption_and_isolation() {
    run_scenario(0).await;
}
#[tokio::test(start_paused = true)]
async fn core_task_03_minimum_capacity_and_snapshot_bypass() {
    run_scenario(1).await;
}
#[tokio::test(start_paused = true)]
#[should_panic]
async fn core_task_03_invalid_take_type_panics() {
    StoreRateLimit::new(1.0)
        .take(1, Type::from_i64(-1), PriorityLevel::LOW)
        .await;
}
#[tokio::test(start_paused = true)]
#[should_panic]
async fn core_task_03_invalid_available_type_panics() {
    StoreRateLimit::new(1.0)
        .available(1, Type::from_i64(4), PriorityLevel::LOW)
        .await;
}
