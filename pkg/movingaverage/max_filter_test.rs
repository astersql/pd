// Copyright 2026 AsterSQL.

use crate::MaxFilter;

#[test]
fn max_filter_go_sequence_set_reset() {
    let data = [2., 1., 3., 4., 1., 1., 3., 3., 2., 0., 5.];
    let expected = [2., 2., 3., 4., 4., 4., 4., 4., 3., 3., 5.];
    let mut filter = MaxFilter::new(5);
    assert_eq!(filter.get(), 0.);
    assert_eq!(filter.get_instantaneous(), 0.);
    for use_set in [false, true] {
        for value in 0..100 {
            filter.add(value as f64);
        }
        if use_set {
            filter.set(data[0]);
        } else {
            filter.reset();
        }
        if !use_set {
            assert_eq!(filter.get(), 0.);
            assert_eq!(filter.get_instantaneous(), 0.);
        } else {
            assert_eq!(filter.get(), data[0]);
            assert_eq!(filter.get_instantaneous(), data[0]);
        }
        for i in usize::from(use_set)..data.len() {
            filter.add(data[i]);
            assert!((filter.get() - expected[i]).abs() <= 1e-7);
            assert_eq!(filter.get_instantaneous(), data[i]);
        }
    }
}

#[test]
fn max_filter_negative_values_and_window_expiry() {
    for size in [1, 2, 5, 17] {
        let mut filter = MaxFilter::new(size);
        let mut history = Vec::new();
        for i in 0..200 {
            let value = -((i * 17 % 31) as f64) - 0.5;
            history.push(value);
            filter.add(value);
            let start = history.len().saturating_sub(size as usize);
            let expected = history[start..]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            assert_eq!(filter.get(), expected);
            assert_eq!(filter.get_instantaneous(), value);
        }
        filter.reset();
        filter.add(-100.);
        assert_eq!(filter.get(), -100.);
    }
}

#[test]
fn max_filter_ieee_comparison_order() {
    let mut filter = MaxFilter::new(3);
    filter.add(f64::NAN);
    filter.add(5.);
    assert!(filter.get().is_nan());
    filter.add(f64::INFINITY);
    filter.add(-1.);
    assert_eq!(filter.get(), f64::INFINITY);
    filter.set(-0.);
    filter.add(0.);
    assert_eq!(filter.get().to_bits(), (-0.0f64).to_bits());
    filter.set(2.);
    filter.add(f64::NAN);
    assert_eq!(filter.get(), 2.);
    assert!(filter.get_instantaneous().is_nan());
}

#[test]
fn max_filter_invalid_and_empty_windows() {
    assert!(std::panic::catch_unwind(|| MaxFilter::new(-1)).is_err());
    let mut filter = MaxFilter::new(0);
    assert_eq!(filter.get(), 0.);
    assert_eq!(filter.get_instantaneous(), 0.);
    filter.reset();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| filter.add(1.))).is_err());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| filter.set(1.))).is_err());
}
