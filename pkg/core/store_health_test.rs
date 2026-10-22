// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn core_task_23_space() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_23.json")).unwrap();
    for c in fixture["space"].as_array().unwrap() {
        let mut s = StoreInfo::new(None);
        s.set_region_count(c["regions"].as_i64().unwrap());
        s.update_raw_stats(StoreStats {
            capacity: c["capacity"].as_u64().unwrap(),
            available: c["available"].as_u64().unwrap(),
            used_size: 123,
            ..Default::default()
        });
        assert_eq!(s.storage_size(), 123);
        assert_eq!(s.available_ratio(), c["ratio"].as_f64().unwrap());
        assert_eq!(s.is_low_space(0.8), c["low"].as_bool().unwrap());
    }
}
#[test]
fn core_task_23_slow() {
    let mut s = StoreInfo::new(None);
    for score in [0, 79, 80, u64::MAX] {
        s.update_raw_stats(StoreStats {
            slow_score: score,
            network_slow_scores: [(9, score)].into(),
            ..Default::default()
        });
        assert_eq!(s.slow_score(), score);
        assert_eq!(s.is_slow(), score >= 80);
        let mut scores = s.network_slow_scores();
        scores.clear();
        assert_eq!(s.network_slow_scores().get(&9), Some(&score));
    }
    s.update_raw_stats(StoreStats::default());
    s.slow_trend_evicted();
    assert!(s.is_slow());
    s.slow_trend_recovered();
    assert!(!s.is_slow());
}
#[test]
fn core_task_23_resolved() {
    for state in [
        NodeState::Preparing,
        NodeState::Serving,
        NodeState::Removing,
        NodeState::Removed,
    ] {
        for engine in ["", "tikv", "tiflash", "tiflash_compute", "unknown"] {
            for leaders in [0, 1, -1] {
                let mut s = StoreInfo::new(StoreMeta {
                    node_state: state,
                    labels: vec![StoreLabel {
                        key: "engine".into(),
                        value: engine.into(),
                    }],
                    ..Default::default()
                });
                s.set_leader_count(leaders);
                s.set_min_resolved_ts(u64::MAX);
                assert_eq!(s.min_resolved_ts(), u64::MAX);
                assert_eq!(s.clone().min_resolved_ts(), u64::MAX);
                assert_eq!(
                    s.is_available_for_min_resolved_ts(),
                    state != NodeState::Removed
                        && (engine == "" || engine == "tikv")
                        && leaders != 0
                );
            }
        }
    }
}

#[test]
fn core_task_68_witness_score_delta_and_wrapping() {
    // store.go:WitnessScore: signed integer addition precedes float conversion.
    let mut store = StoreInfo::new(None);
    for (count, delta, expected) in [
        (0, 0, 0.0),
        (4, -2, 2.0),
        (0, -1, -1.0),
        (i64::MAX, 1, i64::MIN as f64),
        (i64::MIN, -1, i64::MAX as f64),
    ] {
        store.set_witness_count(count);
        assert_eq!(store.witness_score(delta), expected);
    }
}
