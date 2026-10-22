// Copyright 2026 AsterSQL.

use crate::{RollingCounter, RollingCounterOpts, count};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[test]
fn rolling_counter_aggregates() {
    let start = Instant::now();
    let now = Arc::new(Mutex::new(start));
    let clock = Arc::clone(&now);
    let r = RollingCounter::with_clock(
        RollingCounterOpts {
            size: 3,
            bucket_duration: Duration::from_secs(1),
        },
        move || *clock.lock().unwrap(),
    );
    assert!(r.avg().is_nan());
    assert_eq!(
        (r.sum(), r.min(), r.max(), r.value(), r.timespan()),
        (0.0, 0.0, 0.0, 0, 0)
    );
    let buckets = || {
        let mut values = Vec::new();
        r.reduce(|mut i| {
            while i.has_next() {
                values.push(i.bucket().points.clone());
            }
            0.0
        });
        values
    };
    assert_eq!(buckets(), vec![vec![], vec![], vec![]]);
    r.add(1);
    assert_eq!(buckets(), vec![vec![], vec![], vec![1.0]]);
    *now.lock().unwrap() = start + Duration::from_secs(1);
    r.add(2);
    r.add(3);
    *now.lock().unwrap() = start + Duration::from_secs(2);
    r.add(4);
    r.add(5);
    r.add(6);
    *now.lock().unwrap() = start + Duration::from_secs(3);
    r.add(7);
    assert_eq!(buckets(), vec![vec![5.0], vec![15.0], vec![7.0]]);
    assert_eq!(
        (r.sum(), r.min(), r.max(), r.avg(), r.value()),
        (27.0, 5.0, 15.0, 9.0, 27)
    );
    assert_eq!(r.reduce(count), 6.0);
    *now.lock().unwrap() = start + Duration::from_secs(4);
    assert_eq!(r.timespan(), 1);
    assert_eq!(buckets(), vec![vec![15.0], vec![7.0]]);
    assert_eq!(r.sum(), 22.0);
    *now.lock().unwrap() = start + Duration::from_secs(6);
    assert_eq!(r.timespan(), 3);
    assert_eq!(
        (r.sum(), r.avg(), r.min(), r.max(), r.value()),
        (0.0, 0.0, 0.0, 0.0, 0)
    );
    assert_eq!(r.reduce(|_| panic!("expired reducer must not run")), 0.0);
    *now.lock().unwrap() = start + Duration::from_secs(13);
    assert_eq!(r.timespan(), 10);
    r.add(-9);
    r.add(2);
    assert_eq!(
        (r.sum(), r.min(), r.max(), r.avg(), r.value()),
        (-7.0, -7.0, -7.0, -7.0, -7)
    );
    *now.lock().unwrap() = start + Duration::from_millis(12_500);
    assert_eq!(r.timespan(), 0);
    *now.lock().unwrap() = start + Duration::from_secs(12);
    assert_eq!(r.timespan(), 3);
    assert_eq!(r.sum(), 0.0);
    let _ = RollingCounter::new(RollingCounterOpts {
        size: 1,
        bucket_duration: Duration::from_secs(1),
    });
}

#[test]
fn rolling_counter_aggregates_concurrent() {
    let start = Instant::now();
    let r = RollingCounter::with_clock(
        RollingCounterOpts {
            size: 3,
            bucket_duration: Duration::from_secs(1),
        },
        move || start,
    );
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..1000 {
                    r.add(1);
                    assert!(r.sum() >= 1.0);
                }
            });
        }
    });
    assert_eq!(r.value(), 4000);
    assert_eq!(r.reduce(count), 4000.0);
    assert_eq!(r.avg(), 4000.0);
}

#[test]
fn rolling_counter_aggregates_integer_conversion() {
    let r = RollingCounter::new(RollingCounterOpts {
        size: 3,
        bucket_duration: Duration::from_secs(3600),
    });
    r.add(i64::MAX); // Go first rounds the integer to f64 (2^63).
    assert_eq!(r.sum(), 9_223_372_036_854_775_808.0);
    assert_eq!(r.value(), i64::MIN);
    r.add(i64::MIN);
    assert_eq!(r.value(), 0);
    r.add(-17);
    assert_eq!(r.value(), -17);
}
