// Copyright 2026 AsterSQL.

use serde::Deserialize;

use crate::Window;

#[derive(Deserialize)]
struct Scenario {
    name: String,
    capacity: i64,
    operations: Vec<(String, i64)>,
    expected: Vec<State>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct State {
    capacity: i64,
    used: i64,
    count: u64,
    available: bool,
    result: i64,
}

fn check(name: &str) {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_05.json")).unwrap();
    let s = scenarios.into_iter().find(|s| s.name == name).unwrap();
    let mut w = Window::new(s.capacity);
    assert_eq!(s.expected.len(), s.operations.len());
    for ((action, value), expected) in s.operations.into_iter().zip(s.expected) {
        let result = match action.as_str() {
            "take" => i64::from(w.take(value)),
            "ack" => w.ack(value),
            "reset" => {
                w.reset(value);
                0
            }
            _ => panic!("unknown action {action}"),
        };
        assert_eq!(
            State {
                capacity: w.capacity(),
                used: w.used(),
                count: w.count(),
                available: w.available(),
                result
            },
            expected,
            "{name}: {action}({value})"
        );
    }
}

#[test]
fn core_task_05_minimum_and_overshoot() {
    check("minimum_and_overshoot");
}

#[test]
fn core_task_05_reset_preserves_used() {
    check("reset_preserves_used");
}

#[test]
fn core_task_05_boundary() {
    check("boundary");
}
