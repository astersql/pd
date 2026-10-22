// Copyright 2026 AsterSQL.
use crate::ConcurrencyLimiter;
use std::sync::{Arc, Barrier};

#[test]
fn concurrency_allow_release() {
    let limiter = ConcurrencyLimiter::new(10);
    for _ in 0..10 {
        assert!(limiter.allow());
    }
    assert!(!limiter.allow());
    limiter.release();
    assert!(limiter.allow());
    assert_eq!(limiter.get_limit(), 10);
    assert_eq!(limiter.get_max_concurrency(), 10);
    assert_eq!(limiter.get_max_concurrency(), 10);
    limiter.set_limit(5);
    assert_eq!(limiter.get_limit(), 5);
    assert_eq!(limiter.get_running_tasks_num(), 10);
    assert!(!limiter.allow());
    for _ in 0..6 {
        limiter.release();
    }
    assert!(limiter.allow());
    assert!(!limiter.allow());
    for _ in 0..6 {
        limiter.release();
    }
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_max_concurrency(), 10);
    for _ in 0..5 {
        assert!(limiter.allow());
    }
    for _ in 0..5 {
        limiter.release();
    }
    assert_eq!(limiter.get_max_concurrency(), 5);
    assert_eq!(limiter.get_max_concurrency(), 0);
    limiter.set_limit(0);
    for _ in 0..20 {
        assert!(limiter.allow());
    }
    assert_eq!(limiter.get_max_concurrency(), 20);
    limiter.set_limit(1);
    assert!(!limiter.allow());
    for _ in 0..20 {
        limiter.release();
    }
    assert!(limiter.allow());
}

#[test]
fn concurrency_allow_release_parallel() {
    for limit in [0, 7] {
        let limiter = Arc::new(ConcurrencyLimiter::new(limit));
        let barrier = Arc::new(Barrier::new(33));
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..32 {
                let limiter = Arc::clone(&limiter);
                let barrier = Arc::clone(&barrier);
                handles.push(scope.spawn(move || {
                    barrier.wait();
                    let allowed = limiter.allow();
                    barrier.wait();
                    barrier.wait();
                    if allowed {
                        limiter.release();
                    }
                    allowed
                }));
            }
            barrier.wait();
            barrier.wait();
            let expected = if limit == 0 { 32 } else { limit };
            assert_eq!(limiter.get_running_tasks_num(), expected);
            assert_eq!(limiter.get_max_concurrency(), expected);
            barrier.wait();
            let admitted = handles
                .into_iter()
                .filter_map(|h| h.join().unwrap().then_some(1))
                .sum::<u64>();
            assert_eq!(admitted, expected);
        });
        assert_eq!(limiter.get_running_tasks_num(), 0);
        assert_eq!(
            limiter.get_max_concurrency(),
            if limit == 0 { 32 } else { limit }
        );
        assert_eq!(limiter.get_max_concurrency(), 0);
    }
}
