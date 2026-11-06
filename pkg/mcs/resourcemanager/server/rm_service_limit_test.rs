// Copyright 2026 AsterSQL.
use super::ServiceLimit;
#[test]
fn mcs_096_shared_budget_refill_and_backwards_clock() {
    let mut limit = ServiceLimit::new(100.0, 0);
    assert_eq!(limit.apply(1_000_000_000, 60.0), (60.0, 600));
    assert_eq!(limit.apply(1_000_000_000, 60.0), (40.0, 400));
    assert_eq!(limit.apply(1_000_000_000, 1.0), (0.0, 0));
    assert_eq!(limit.apply(6_000_000_000, 1000.0), (500.0, 5000));
    limit.refill(5_000_000_000);
    assert_eq!(limit.last_update, 6_000_000_000);
    assert_eq!(limit.available, 0.0);
    assert_eq!(limit.apply(6_500_000_000, 1000.0), (50.0, 500));
}
#[test]
fn mcs_096_updates_cap_and_clear_prior_capacity() {
    let mut limit = ServiceLimit::new(100.0, 0);
    limit.refill(5_000_000_000);
    assert_eq!(limit.available, 500.0);
    assert!(limit.set(20.0, 6_000_000_000));
    assert_eq!(limit.available, 100.0);
    let before = limit.last_update;
    assert!(!limit.set(20.0, 7_000_000_000));
    assert_eq!(limit.last_update, before);
    assert!(limit.set(0.0, 7_000_000_000));
    assert_eq!(limit.available, 0.0);
    assert_eq!(limit.apply(7_000_000_000, 1000.0), (1000.0, 0));
    assert!(limit.set(10.5, 8_000_000_000));
    assert_eq!(limit.available, 0.0);
    assert_eq!(limit.apply(8_500_000_000, 7.75), (5.25, 500));
    assert!(limit.set(-1.0, 9_000_000_000));
    assert_eq!(limit.rate, 0.0);
    assert_eq!(limit.available, 0.0);
}
