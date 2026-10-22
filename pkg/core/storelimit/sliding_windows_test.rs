// Copyright 2026 AsterSQL.
use crate::{SlidingWindows, Type, VERSION_V2};
use astersql_pd_core_constant::PriorityLevel;
use serde::Deserialize;

#[derive(Deserialize)]
struct Operation {
    action: String,
    token: i64,
    typ: i64,
    level: i32,
    used: [i64; 4],
    result: bool,
}

#[test]
fn core_task_06_priority_snapshot_oracle() {
    let ops: Vec<Operation> =
        serde_json::from_str(include_str!("rust-fixtures/core_task_06.json")).unwrap();
    let mut windows = SlidingWindows::new();
    assert_eq!(windows.version(), VERSION_V2);
    assert_eq!(windows.capacity(), 100);
    for (i, op) in ops.iter().enumerate() {
        let typ = Type::from_i64(op.typ);
        let level = PriorityLevel::from_i32(op.level);
        let result = match op.action.as_str() {
            "take" => windows.take(op.token, typ, level),
            "available" => windows.available(op.token, typ, level),
            "ack" => {
                windows.ack(op.token, typ);
                false
            }
            _ => panic!("unknown action"),
        };
        assert_eq!(result, op.result, "step {i}");
        assert_eq!(windows.used(), op.used, "step {i}");
    }
}
