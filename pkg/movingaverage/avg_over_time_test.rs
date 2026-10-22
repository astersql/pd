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

#[test]
fn avg_over_time_state_set_clear_clone() {
    let mut avg = AvgOverTime::new(5 * SECOND);
    assert_eq!(avg.get_instantaneous(), 0.0);
    avg.add(1000.0, SECOND);
    assert_eq!(avg.get_instantaneous(), 1000.0);
    avg.set(10.0);
    assert!(avg.is_full());
    assert_eq!(avg.get(), 10.0);
    assert_eq!(avg.get_interval_sum(), 5 * SECOND);
    assert_eq!(avg.get_instantaneous(), 10.0);
    let mut copy = avg.clone();
    copy.add(20.0, SECOND);
    // Go Set stores the seed in both the margin and queue.
    assert_eq!(copy.get(), 12.0);
    assert_eq!(copy.get_instantaneous(), 20.0);
    assert_eq!(avg.get(), 10.0);
    copy.clear();
    assert!(!copy.is_full());
    assert_eq!(copy.get_interval_sum(), 0);
    assert_eq!(copy.get(), 0.0);
    assert_eq!(copy.get_instantaneous(), 0.0);
    copy.add(25.0, 5 * SECOND);
    assert_eq!(copy.get_instantaneous(), 5.0);
    assert_eq!(copy.get(), 5.0);

    let mut queued = AvgOverTime::new(5 * SECOND);
    queued.add(10.0, SECOND);
    queued.add(40.0, 2 * SECOND);
    let mut clone = queued.clone();
    clone.add(60.0, 3 * SECOND);
    assert_eq!(clone.get(), 20.0);
    assert_eq!(queued.get(), 0.0);
    assert_eq!(queued.get_interval_sum(), 3 * SECOND);
    assert_eq!(queued.get_instantaneous(), 20.0);
}

#[test]
fn avg_over_time_state_unstable_and_signed_intervals() {
    let mut avg = AvgOverTime::new(5 * SECOND);
    avg.add(5000.0, 5 * SECOND);
    for i in 0..1000 {
        let interval = (i % 5) * SECOND;
        avg.add(1000.0 * (i % 5) as f64, interval);
        assert!((avg.get() - 1000.0).abs() < 1e-10);
    }
    let before = (avg.get(), avg.get_instantaneous(), avg.get_interval_sum());
    avg.add(f64::NAN, 0);
    assert_eq!(
        before,
        (avg.get(), avg.get_instantaneous(), avg.get_interval_sum())
    );
    avg.clear();
    avg.add(10.0, -SECOND);
    assert_eq!(avg.get(), 0.0);
    assert_eq!(avg.get_interval_sum(), -SECOND);
    assert_eq!(avg.get_instantaneous(), -10.0);
    avg.add(60.0, 6 * SECOND);
    assert_eq!(avg.get(), 10.0);
    assert_eq!(avg.get_instantaneous(), 10.0);
    let zero = AvgOverTime::new(0);
    assert!(zero.is_full());
    assert!(zero.get().is_nan());
    let negative = AvgOverTime::new(-SECOND);
    assert!(negative.is_full());
    assert!(negative.get().is_nan());
}
