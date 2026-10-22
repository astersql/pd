// Copyright 2026 AsterSQL.

use crate::AvgOverTime;

const SECOND: i64 = 1_000_000_000;

#[test]
fn avg_over_time_window_min_filled_and_pulse() {
    let mut avg = AvgOverTime::new(5 * SECOND);
    assert_eq!(avg.get(), 0.0);
    assert!(!avg.is_full());
    for _ in 0..4 {
        avg.add(1000.0, SECOND);
        assert_eq!(avg.get(), 0.0);
        assert!(!avg.is_full());
    }
    avg.add(1000.0, SECOND);
    assert_eq!(avg.get(), 1000.0);
    assert!(avg.is_full());
    for i in 0..110 {
        avg.add(if i % 2 == 0 { 1000.0 } else { 0.0 }, SECOND);
        if i >= 10 {
            assert!((400.0..=600.0).contains(&avg.get()));
        }
    }
}

#[test]
fn avg_over_time_window_fractional_margin_and_eviction() {
    let mut avg = AvgOverTime::new(5 * SECOND);
    for (delta, seconds, expected) in [
        (60.0, 6, 10.0),
        (20.0, 1, 12.0),
        (30.0, 1, 16.0),
        (40.0, 1, 22.0),
        (50.0, 1, 30.0),
        (60.0, 1, 40.0),
        (700.0, 7, 100.0),
    ] {
        avg.add(delta, seconds * SECOND);
        assert!((avg.get() - expected).abs() < 1e-10);
    }
    let mut fractional = AvgOverTime::new(SECOND / 2);
    fractional.add(6.0, SECOND / 4);
    assert_eq!(fractional.get(), 0.0);
    fractional.add(12.0, SECOND / 2);
    assert_eq!(fractional.get(), 24.0);
    fractional.add(-2.0, SECOND / 4);
    assert_eq!(fractional.get(), 8.0);
}
