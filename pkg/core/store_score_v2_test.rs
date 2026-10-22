// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreStats};

#[test]
fn core_task_22_fixture_and_routing() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_22.json")).unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut s = StoreInfo::default();
        s.update_raw_stats(StoreStats {
            capacity: c["capacity"].as_u64().unwrap(),
            available: c["available"].as_u64().unwrap(),
            used_size: c["used"].as_u64().unwrap(),
            ..Default::default()
        });
        s.set_region_size(c["size"].as_i64().unwrap());
        s.set_region_weight(c["weight"].as_f64().unwrap());
        let d = c["delta"].as_i64().unwrap();
        let low = c["low"].as_f64().unwrap();
        let actual = s.region_score("v2", 0.7, low, d);
        let expected = c["expected"].as_f64().unwrap();
        assert!(
            (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
            "{c}: {actual}"
        );
        assert_eq!(actual, s.region_score_v2(d, low));
        for version in ["v1", "", "V2", "unknown"] {
            assert_eq!(
                s.region_score(version, 0.7, low, d),
                s.region_score_v1(0.7, low, d)
            );
        }
    }
}

fn disk(capacity: u64, available: u64, amp: f64) -> StoreInfo {
    let mut s = StoreInfo::default();
    s.update_raw_stats(StoreStats {
        capacity: capacity * 1073741824,
        available: available * 1073741824,
        used_size: (capacity - available) * 1073741824,
        ..Default::default()
    });
    s.set_region_size(((capacity - available) as f64 * 1024.0 * amp) as i64);
    s
}

#[test]
fn core_task_22_existing_go_ordering() {
    // TestLowSpaceScoreV2's first eight pairs; the ninth preserves unrelated files.
    for (b, s) in [
        ((100, 20, 1.4), (1000, 200, 1.4)),
        ((1000, 10, 1.4), (100, 10, 1.4)),
        ((1000, 20, 1.4), (500, 10, 1.4)),
        ((100, 10, 1.4), (1000, 100, 1.4)),
        ((100, 10, 1.5), (100, 10, 1.4)),
        ((100, 60, 1.0), (100, 80, 2.0)),
        ((100, 80, 3.0), (100, 60, 1.0)),
        ((100, 2, 3.0), (10000, 100, 3.0)),
    ] {
        assert!(
            disk(b.0, b.1, b.2).region_score_v2(0, 0.8)
                > disk(s.0, s.1, s.2).region_score_v2(0, 0.8)
        );
    }
    let mut small = disk(7000, 6930, 1.0);
    small.update_raw_stats(StoreStats {
        capacity: 7000 * 1073741824,
        available: 6930 * 1073741824,
        used_size: 400 * 1048576,
        ..Default::default()
    });
    small.set_region_size(400);
    assert!(
        disk(7000, 1500, 1.32).region_score_v2(-37794, 0.8) > small.region_score_v2(37794, 0.8)
    );
    let bigger = disk(3000, 2000, 3.0).region_score_v2(-80000, 0.8);
    assert!(disk(3000, 2900, 0.03).region_score_v2(80000, 0.8) < bigger);
    assert!(disk(3000, 2000, 3.0).region_score_v2(80000, 0.8) > bigger);
}

#[test]
fn core_task_22_average_nan_and_wrapping() {
    let mut s = StoreInfo::new(None);
    s.set_region_size(512000);
    s.update_raw_stats(StoreStats {
        capacity: 1000 * 1073741824,
        available: 500 * 1073741824,
        used_size: 500 * 1073741824,
        ..Default::default()
    });
    s.update_raw_stats(StoreStats {
        capacity: 1000 * 1073741824,
        available: 100 * 1073741824,
        used_size: 500 * 1073741824,
        ..Default::default()
    });
    assert_ne!(s.avg_available(), s.available());
    let average = s.avg_available();
    let mut reference = StoreInfo::default();
    reference.update_raw_stats(StoreStats {
        capacity: s.capacity(),
        available: average,
        used_size: s.used_size(),
        ..Default::default()
    });
    reference.set_region_size(s.region_size());
    reference.set_region_weight(s.region_weight());
    assert_eq!(
        s.region_score_v2(100, 0.8),
        reference.region_score_v2(100, 0.8)
    );
    assert!(s.region_score_v2(0, f64::NAN).is_nan());
    s.set_region_weight(f64::NAN);
    assert!(s.region_score_v2(0, 0.8).is_nan());
    let mut empty = StoreInfo::default();
    empty.set_region_size(i64::MAX);
    empty.set_region_weight(1.0);
    assert_eq!(empty.region_score_v2(1, 0.8), i64::MAX as f64);
}
