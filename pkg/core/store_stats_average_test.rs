// Copyright 2026 AsterSQL.
use super::store_stats_average::climp0;
use super::{StoreInfo, StoreStats};
use serde::Deserialize;

#[derive(Deserialize)]
struct Scenario {
    name: String,
    samples: Vec<u64>,
    expected: Vec<u64>,
}
#[derive(Deserialize)]
struct Fixture {
    scenarios: Vec<Scenario>,
}

#[test]
fn core_task_15_sequences() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("rust-fixtures/core_task_15.json")).unwrap();
    for scenario in fixture.scenarios {
        let mut store = StoreInfo::new(None);
        assert_eq!(store.avg_available(), 0);
        for (sample, expected) in scenario.samples.into_iter().zip(scenario.expected) {
            store.update_raw_stats(StoreStats {
                available: sample,
                ..Default::default()
            });
            assert_eq!(store.available(), sample);
            assert_eq!(store.avg_available(), expected, "{}", scenario.name);
        }
    }
}

#[test]
fn core_task_15_clamp() {
    for (input, expected) in [(-12.5, 0), (0.0, 0), (0.9, 0), (12.9, 12)] {
        assert_eq!(climp0(input), expected);
    }
}

#[test]
fn core_task_15_unsmoothed_zero_value() {
    let mut store = StoreInfo::default();
    store.update_raw_stats(StoreStats {
        available: 123,
        capacity: 0,
        ..Default::default()
    });
    assert_eq!(store.avg_available(), 123);
}

#[test]
fn core_task_15_window_rollover_and_negative_overshoot() {
    let mut store = StoreInfo::new(None);
    for _ in 0..70 {
        store.update_raw_stats(StoreStats {
            available: 1000,
            ..Default::default()
        });
    }
    assert_eq!(store.avg_available(), 1000);
    for _ in 0..30 {
        store.update_raw_stats(StoreStats::default());
    }
    assert_eq!(store.avg_available(), 0);
    for _ in 0..70 {
        store.update_raw_stats(StoreStats::default());
    }
    assert_eq!(store.avg_available(), 0);
}
