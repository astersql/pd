// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreStats};

#[test]
fn core_task_21_fixture_scores() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_21.json")).unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut s = StoreInfo::default();
        s.update_raw_stats(StoreStats {
            capacity: c["capacity"].as_u64().unwrap() * 1048576,
            available: c["available"].as_u64().unwrap() * 1048576,
            used_size: c["used"].as_u64().unwrap() * 1048576,
            ..Default::default()
        });
        s.set_region_size(c["size"].as_i64().unwrap());
        s.set_region_weight(c["weight"].as_f64().unwrap());
        let actual = s.region_score_v1(0.5, 0.75, c["delta"].as_i64().unwrap());
        let expected = c["expected"].as_f64().unwrap();
        assert!(
            (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
            "{c}: {actual}"
        );
    }
}

#[test]
fn core_task_21_zero_used_regression_and_nan_weight() {
    let mut s = StoreInfo::default();
    s.update_raw_stats(StoreStats {
        capacity: 512 * 1048576,
        available: 100 * 1048576,
        ..Default::default()
    });
    s.set_region_size(1);
    assert!(!s.region_score_v1(0.7, 0.9, 0).is_nan());
    s.set_region_weight(f64::NAN);
    assert!(s.region_score_v1(0.7, 0.9, 0).is_nan());
}
