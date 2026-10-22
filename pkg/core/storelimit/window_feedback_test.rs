// Copyright 2026 AsterSQL.

use crate::{SlidingWindows, Type};
use astersql_pd_core_constant::PriorityLevel;
use serde::Deserialize;

#[derive(Deserialize)]
struct Operation {
    action: String,
    #[serde(default)]
    token: i64,
    #[serde(default)]
    error: f64,
    capacity: i64,
    used: [i64; 4],
}

#[test]
fn core_task_07_go_feedback_sequence() {
    let ops: Vec<Operation> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_07.json")).unwrap();
    let mut windows = SlidingWindows::new();
    for op in ops {
        match op.action.as_str() {
            "take" => assert!(windows.take(op.token, Type::SEND_SNAPSHOT, PriorityLevel::LOW)),
            "feedback" => windows.feedback(op.error),
            _ => panic!("unknown operation"),
        }
        assert_eq!(windows.capacity(), op.capacity);
        assert_eq!(windows.used(), op.used);
    }
}

#[test]
fn core_task_07_all_priority_capacities_and_usage_survive_feedback() {
    let mut windows = SlidingWindows::new();
    assert!(windows.take(100, Type::SEND_SNAPSHOT, PriorityLevel::LOW));
    windows.feedback(10.0);
    assert!(windows.take(200, Type::SEND_SNAPSHOT, PriorityLevel::LOW));
    assert!(!windows.available(1, Type::SEND_SNAPSHOT, PriorityLevel::LOW));
    assert!(windows.take(150, Type::SEND_SNAPSHOT, PriorityLevel::MEDIUM));
    assert!(windows.take(75, Type::SEND_SNAPSHOT, PriorityLevel::HIGH));
    assert!(windows.take(37, Type::SEND_SNAPSHOT, PriorityLevel::URGENT));
    assert!(!windows.available(1, Type::SEND_SNAPSHOT, PriorityLevel::URGENT));
    windows.feedback(-100.0);
    assert_eq!(windows.capacity(), 100);
    assert_eq!(windows.used(), [300, 150, 75, 37]);
}
