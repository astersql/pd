// Copyright 2026 AsterSQL.
use crate::StoreInfo;
use astersql_pd_core_constant::Direction;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    kind: String,
    ops: Vec<i32>,
    expected: Vec<bool>,
}

fn apply(store: &mut StoreInfo, kind: &str, op: i32) {
    match (kind, op) {
        ("in", 1) => store.pause_leader_transfer(Direction::IN),
        ("in", _) => store.resume_leader_transfer(Direction::IN),
        ("out", 1) => store.pause_leader_transfer(Direction::OUT),
        ("out", _) => store.resume_leader_transfer(Direction::OUT),
        ("slow", 1) => store.slow_store_evicted(),
        ("slow", _) => store.slow_store_recovered(),
        ("stopping", 1) => store.stopping_store_evicted(),
        ("stopping", _) => store.stopping_store_recovered(),
        ("trend", 1) => store.slow_trend_evicted(),
        ("trend", _) => store.slow_trend_recovered(),
        _ => unreachable!(),
    }
}
fn state(store: &StoreInfo) -> Vec<bool> {
    vec![
        store.allow_leader_transfer_in(),
        store.allow_leader_transfer_out(),
        store.evicted_as_slow_store(),
        store.evicted_as_stopping_store(),
        store.is_evicted_as_slow_trend(),
    ]
}
#[test]
fn core_task_18_source_scenarios() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_18.json")).unwrap();
    for scenario in fixture.scenarios {
        let mut store = StoreInfo::default();
        let index = ["in", "out", "slow", "stopping", "trend"]
            .iter()
            .position(|k| *k == scenario.kind)
            .unwrap();
        for (op, expected) in scenario.ops.into_iter().zip(scenario.expected) {
            apply(&mut store, &scenario.kind, op);
            let mut all = vec![true, true, false, false, false];
            all[index] = expected;
            assert_eq!(state(&store), all, "{} delta {}", scenario.kind, op);
        }
    }
}
#[test]
fn core_task_18_invalid_direction_is_noop() {
    let mut store = StoreInfo::new(None);
    for value in [-1, 2, i32::MAX] {
        store.pause_leader_transfer(Direction::from_i32(value));
        store.resume_leader_transfer(Direction::from_i32(value));
    }
    assert_eq!(state(&store), vec![true, true, false, false, false]);
}
#[test]
fn core_task_18_clone_counters_are_independent() {
    let mut original = StoreInfo::new(None);
    for kind in ["in", "out", "slow", "stopping", "trend"] {
        apply(&mut original, kind, 1);
    }
    for mut copy in [original.clone(), original.shallow_clone()] {
        assert_eq!(state(&copy), state(&original));
        for kind in ["in", "out", "slow", "stopping", "trend"] {
            apply(&mut copy, kind, -1);
        }
        assert_eq!(state(&copy), vec![true, true, false, false, false]);
    }
    assert_eq!(state(&original), vec![false, false, true, true, true]);
}
