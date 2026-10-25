// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_storelimit::{TYPE_NAME_VALUE, Type};
use std::sync::{Arc, Barrier};

#[test]
fn task_017_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_017.json"
    ))
    .unwrap();
    for case in fixture["cost_cases"].as_array().unwrap() {
        for typ in [Type::ADD_PEER, Type::REMOVE_PEER] {
            let mut s = StoreInfluence::default();
            assert_eq!(s.get_step_cost(typ), 0);
            s.adjust_step_cost(typ, case[0].as_i64().unwrap(), case[1].as_i64().unwrap());
            assert_eq!(s.get_step_cost(typ), case[2].as_i64().unwrap());
            s.add_step_cost(typ, -case[2].as_i64().unwrap());
            assert_eq!(s.get_step_cost(typ), 0);
        }
    }
    let s = StoreInfluence {
        region_size: 10,
        region_count: 2,
        leader_size: 30,
        leader_count: 4,
        witness_count: 5,
        ..Default::default()
    };
    for c in fixture["property_cases"].as_array().unwrap() {
        assert_eq!(
            s.resource_property(ScheduleKind::new(
                ResourceKind::from_i32(c[0].as_i64().unwrap() as i32),
                SchedulePolicy::from_i32(c[1].as_i64().unwrap() as i32)
            )),
            c[2].as_i64().unwrap()
        );
    }
    for typ in [Type::SEND_SNAPSHOT, Type::TRANSFER_LEADER_IN] {
        let mut s = StoreInfluence::default();
        s.adjust_step_cost(typ, 21, 0);
        assert_eq!(s.get_step_cost(typ), 1000);
        // Empty regions never index the small-region cost table.
        s.adjust_step_cost(typ, 1, 0);
        assert_eq!(s.get_step_cost(typ), 1000);
    }
}

#[test]
fn task_017_aggregation_and_snapshot_ownership() {
    let a = OpInfluence::default();
    assert_eq!(a.get_store_influence(99), StoreInfluence::default());
    assert!(a.snapshot().contains_key(&99));
    let b = OpInfluence::default();
    b.update_store(1, |s| {
        s.region_size = 10;
        s.region_count = 2;
        s.leader_size = -3;
        s.leader_count = 4;
        s.witness_count = -5;
        for (_, typ) in TYPE_NAME_VALUE {
            s.add_step_cost(typ, 7);
        }
        s.add_step_cost(Type::from_i64(99), 12);
    });
    a.add(&b);
    a.add(&b);
    let s = a.get_store_influence(1);
    assert_eq!(
        (
            s.region_size,
            s.region_count,
            s.leader_size,
            s.leader_count,
            s.witness_count
        ),
        (20, 4, -6, 8, -10)
    );
    for (_, typ) in TYPE_NAME_VALUE {
        assert_eq!(s.get_step_cost(typ), 14);
    }
    assert_eq!(s.get_step_cost(Type::from_i64(99)), 0);
    let mut copy = a.snapshot();
    copy.get_mut(&1).unwrap().add_step_cost(Type::ADD_PEER, 100);
    assert_eq!(a.get_store_influence(1).get_step_cost(Type::ADD_PEER), 14);
    a.add(&a);
    assert_eq!(a.get_store_influence(1).region_size, 40);
    let negative = OpInfluence::default();
    negative.update_store(1, |s| {
        s.region_size = -40;
        s.region_count = -8;
        s.leader_size = 12;
        s.leader_count = -16;
        s.witness_count = 20;
        for (_, t) in TYPE_NAME_VALUE {
            s.add_step_cost(t, -28);
        }
    });
    a.add(&negative);
    let s = a.get_store_influence(1);
    assert_eq!(
        (
            s.region_size,
            s.region_count,
            s.leader_size,
            s.leader_count,
            s.witness_count
        ),
        (0, 0, 0, 0, 0)
    );
    for (_, t) in TYPE_NAME_VALUE {
        assert_eq!(s.get_step_cost(t), 0);
    }
    assert_eq!(b.get_store_influence(1).region_size, 10);
}

#[test]
fn task_017_concurrent_snapshots() {
    let influence = Arc::new(OpInfluence::default());
    let barrier = Arc::new(Barrier::new(5));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let m = influence.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..250 {
                    m.update_store(1, |s| {
                        s.region_count += 1;
                        s.leader_count += 1;
                        s.add_step_cost(Type::ADD_PEER, 1);
                    });
                    let snap = m.snapshot();
                    let s = &snap[&1];
                    assert_eq!(s.region_count, s.leader_count);
                    assert_eq!(s.region_count, s.get_step_cost(Type::ADD_PEER));
                }
            });
        }
        barrier.wait();
    });
    assert_eq!(influence.get_store_influence(1).region_count, 1000);
}

#[test]
fn task_017_invalid_small_cost_is_programmer_error() {
    for typ in [
        Type::SEND_SNAPSHOT,
        Type::TRANSFER_LEADER_IN,
        Type::from_i64(-1),
        Type::from_i64(99),
    ] {
        let mut s = StoreInfluence::default();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || s.adjust_step_cost(typ, 2, 1)
            ))
            .is_err()
        );
        assert_eq!(s.get_step_cost(typ), 0);
    }
}

#[test]
fn task_017_wrapping_and_reciprocal_adds() {
    let mut s = StoreInfluence {
        region_size: i64::MAX,
        ..Default::default()
    };
    s.add_step_cost(Type::ADD_PEER, i64::MAX);
    let mut delta = StoreInfluence {
        region_size: 1,
        ..Default::default()
    };
    delta.add_step_cost(Type::ADD_PEER, 1);
    s.add(&delta);
    assert_eq!(s.region_size, i64::MIN);
    assert_eq!(s.get_step_cost(Type::ADD_PEER), i64::MIN);
    let a = OpInfluence::new();
    let b = OpInfluence::new();
    a.update_store(1, |s| s.region_count = 1);
    b.update_store(2, |s| s.witness_count = 2);
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            a.add(&b);
        });
        scope.spawn(|| {
            barrier.wait();
            b.add(&a);
        });
    });
    assert!(a.get_store_influence(2).witness_count >= 2);
    assert!(b.get_store_influence(1).region_count >= 1);
}
