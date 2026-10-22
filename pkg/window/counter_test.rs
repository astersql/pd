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

// Go TestRollingCounterDataRace traverses buckets during writes. Also assert
// exact totals at controlled bucket boundaries so lost updates cannot hide.
#[test]
fn rolling_counter_concurrency() {
    use std::sync::{
        Barrier,
        atomic::{AtomicU64, Ordering},
    };

    const WRITERS: usize = 4;
    const READERS: usize = 2;
    const ADDS: usize = 1000;
    const ROUNDS: usize = 8;
    let start = Instant::now();
    let tick = AtomicU64::new(0);
    let counter = RollingCounter::with_clock(
        RollingCounterOpts {
            size: 3,
            bucket_duration: Duration::from_secs(1),
        },
        || start + Duration::from_secs(tick.load(Ordering::SeqCst)),
    );
    let barrier = Barrier::new(WRITERS + READERS + 1);
    std::thread::scope(|scope| {
        for _ in 0..WRITERS {
            let (counter, barrier) = (&counter, &barrier);
            scope.spawn(move || {
                for _ in 0..ROUNDS {
                    barrier.wait();
                    for _ in 0..ADDS {
                        counter.add(1);
                    }
                    barrier.wait();
                }
            });
        }
        let mut readers = Vec::new();
        for _ in 0..READERS {
            let (counter, barrier) = (&counter, &barrier);
            readers.push(scope.spawn(move || {
                let mut valid = true;
                for round in 0..ROUNDS {
                    barrier.wait();
                    // Report failures after joining so an assertion cannot strand
                    // other participants at the next barrier.
                    for _ in 0..ADDS {
                        counter.reduce(|mut iter| {
                            let (mut total, mut samples) = (0.0, 0);
                            while iter.has_next() {
                                let bucket = iter.bucket();
                                total += bucket.points.iter().sum::<f64>();
                                samples += bucket.count;
                            }
                            let baseline = (round.min(2) * WRITERS * ADDS) as f64;
                            valid &= total == samples as f64
                                && total >= baseline
                                && total <= baseline + (WRITERS * ADDS) as f64;
                            total
                        });
                    }
                    barrier.wait();
                }
                valid
            }));
        }
        let mut totals = Vec::new();
        for round in 0..ROUNDS {
            tick.store(round as u64, Ordering::SeqCst);
            barrier.wait();
            barrier.wait();
            totals.push((counter.value(), counter.reduce(count)));
        }
        for reader in readers {
            assert!(reader.join().unwrap());
        }
        for (round, (value, samples)) in totals.into_iter().enumerate() {
            let expected = ((round + 1).min(3) * WRITERS * ADDS) as i64;
            assert_eq!(value, expected);
            assert_eq!(samples, expected as f64);
        }
    });
    tick.store((ROUNDS + 3) as u64, Ordering::SeqCst);
    assert_eq!(
        counter.reduce(|_| panic!("expired reducer must not run")),
        0.0
    );
    counter.add(7);
    assert_eq!(counter.value(), 7);
    assert_eq!(counter.reduce(count), 1.0);
}
