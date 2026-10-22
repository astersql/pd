// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn priority_and_resource_kind() {
    assert_eq!(PriorityLevel::LOW.as_i32(), 0);
    assert_eq!(PriorityLevel::MEDIUM.as_i32(), 1);
    assert_eq!(PriorityLevel::HIGH.as_i32(), 2);
    assert_eq!(PriorityLevel::URGENT.as_i32(), 3);
    assert_eq!(PriorityLevel::LEN.as_i32(), 4);
    assert_eq!(PriorityLevel::LOW.to_string(), "low");
    assert_eq!(PriorityLevel::MEDIUM.to_string(), "medium");
    assert_eq!(PriorityLevel::HIGH.to_string(), "high");
    assert_eq!(PriorityLevel::URGENT.to_string(), "urgent");
    assert_eq!(PriorityLevel::from_i32(99).to_string(), "unknown");

    assert_eq!(ResourceKind::LEADER.as_i32(), 0);
    assert_eq!(ResourceKind::REGION.as_i32(), 1);
    assert_eq!(ResourceKind::WITNESS.as_i32(), 2);
    assert_eq!(ResourceKind::LEN.as_i32(), 3);
    assert_eq!(ResourceKind::LEADER.to_string(), "leader");
    assert_eq!(ResourceKind::REGION.to_string(), "region");
    assert_eq!(ResourceKind::WITNESS.to_string(), "witness");
    assert_eq!(ResourceKind::from_i32(-1).to_string(), "unknown");

    assert_eq!(SchedulePolicy::BY_COUNT.as_i32(), 0);
    assert_eq!(SchedulePolicy::BY_SIZE.as_i32(), 1);
    let kind = ScheduleKind::new(ResourceKind::REGION, SchedulePolicy::BY_SIZE);
    assert_eq!(kind.resource, ResourceKind::REGION);
    assert_eq!(kind.policy, SchedulePolicy::BY_SIZE);
}

#[test]
fn schedule_policy_round_trip() {
    assert_eq!(SchedulePolicy::BY_COUNT.as_i32(), 0);
    assert_eq!(SchedulePolicy::BY_SIZE.as_i32(), 1);
    assert_eq!(SchedulePolicy::BY_COUNT.to_string(), "count");
    assert_eq!(SchedulePolicy::BY_SIZE.to_string(), "size");
    assert_eq!(SchedulePolicy::from_i32(-1).to_string(), "unknown");

    assert_eq!(string_to_schedule_policy("count"), SchedulePolicy::BY_COUNT);
    assert_eq!(string_to_schedule_policy("size"), SchedulePolicy::BY_SIZE);

    let panic = std::panic::catch_unwind(|| string_to_schedule_policy(" count "))
        .expect_err("unknown schedule policies must panic like the Go oracle");
    let message = panic
        .downcast_ref::<String>()
        .expect("the panic payload should preserve the invalid input");
    assert_eq!(message, "invalid schedule policy:  count ");
}

#[test]
fn key_type_round_trip() {
    assert_eq!(KeyType::TABLE.as_i32(), 0);
    assert_eq!(KeyType::RAW.as_i32(), 1);
    assert_eq!(KeyType::TXN.as_i32(), 2);
    assert_eq!(KeyType::TABLE.to_string(), "table");
    assert_eq!(KeyType::RAW.to_string(), "raw");
    assert_eq!(KeyType::TXN.to_string(), "txn");
    assert_eq!(KeyType::from_i32(-1).to_string(), "unknown");

    assert_eq!(string_to_key_type("table"), KeyType::TABLE);
    assert_eq!(string_to_key_type(" raw\n"), KeyType::RAW);
    assert_eq!(string_to_key_type("\ttxn "), KeyType::TXN);
    assert_eq!(string_to_key_type(""), KeyType::TABLE);
    assert_eq!(string_to_key_type(" \n\t"), KeyType::TABLE);

    let panic = std::panic::catch_unwind(|| string_to_key_type(" invalid "))
        .expect_err("unknown key types must panic like the Go oracle");
    let message = panic
        .downcast_ref::<String>()
        .expect("the panic payload should preserve the invalid input");
    assert_eq!(message, "invalid key type:  invalid ");
}
