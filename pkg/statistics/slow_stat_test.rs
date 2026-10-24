// Copyright 2026 AsterSQL.
use crate::{SlowStat, SlowStoresStats};
use std::sync::Arc;

#[test]
fn state_task_26_slow_stat_go_event_replay() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-26.json"
    ))
    .unwrap();
    let stats = SlowStoresStats::new();
    assert!(!stats.exists_slow_stores());
    for (event, step) in fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["steps"].as_array().unwrap().iter().skip(1))
    {
        let id = event[1].as_u64().unwrap();
        match event[0].as_str().unwrap() {
            "observe" => stats.observe_slow_store_status(id, event[2].as_bool().unwrap()),
            "remove" => stats.remove_slow_store_status(id),
            other => panic!("unknown event {other}"),
        }
        assert_eq!(
            stats.exists_slow_stores(),
            step["exists"].as_bool().unwrap(),
            "Go step {}",
            step["step"]
        );
    }
}

#[test]
fn state_task_26_slow_stat_repeated_true_removes() {
    let stats = SlowStat::new();
    for id in [0, 1, u64::MAX] {
        stats.observe_slow_store_status(id, true);
        assert!(stats.exists_slow_stores());
        stats.observe_slow_store_status(id, true);
        assert!(!stats.exists_slow_stores());
        stats.observe_slow_store_status(id, true);
        assert!(stats.exists_slow_stores());
        stats.observe_slow_store_status(id, false);
        stats.observe_slow_store_status(id, false);
        stats.remove_slow_store_status(id);
        stats.remove_slow_store_status(id);
        assert!(!stats.exists_slow_stores());
    }
}

#[test]
fn state_task_26_slow_stat_store_and_container_isolation() {
    let stats = SlowStoresStats::default();
    let other = SlowStat::default();
    stats.observe_slow_store_status(1, true);
    stats.observe_slow_store_status(2, true);
    stats.remove_slow_store_status(99);
    stats.observe_slow_store_status(99, false);
    stats.observe_slow_store_status(1, true);
    assert!(stats.exists_slow_stores());
    stats.remove_slow_store_status(2);
    assert!(!stats.exists_slow_stores());
    assert!(!other.exists_slow_stores());
}

#[test]
fn state_task_26_slow_stat_concurrent_same_store_parity() {
    let stats = Arc::new(SlowStat::new());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let stats = Arc::clone(&stats);
            std::thread::spawn(move || {
                for _ in 0..101 {
                    stats.observe_slow_store_status(7, true);
                    let _ = stats.exists_slow_stores();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(
        !stats.exists_slow_stores(),
        "808 observations must cancel in pairs"
    );
    stats.observe_slow_store_status(7, true);
    assert!(stats.exists_slow_stores());
}
