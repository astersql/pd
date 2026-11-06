// Copyright 2026 AsterSQL.
use super::*;
fn configured_bucket(tokens: f64, fill: u64, burst: i64) -> Bucket {
    Bucket {
        tokens,
        settings: Some(TokenLimits {
            fill_rate: fill,
            burst_limit: burst,
            max_tokens: 0.0,
        }),
    }
}
#[test]
fn mcs_090_refill_and_loan_sequence() {
    // Go TestGroupTokenBucketRequest uses an explicit, fixed time for this sequence.
    let mut bucket = configured_bucket(200000.0, 2000, 20000000);
    let mut runtime = BucketRuntime::default();
    let now = 1_000_000_000;
    for (required, granted, trickle) in [
        (190000.0, 190000.0, 0),
        (11000.0, 11000.0, 2750),
        (35000.0, 35000.0, 10000),
        (60000.0, 22000.0, 10000),
        (3000.0, 3000.0, 10000),
        (12000.0, 10000.0, 10000),
    ] {
        let (result, time) = runtime
            .request(&mut bucket, now, required, 10000, 0)
            .unwrap();
        assert!(
            (result.tokens - granted).abs() < 1e-7,
            "{required}: {result:?}"
        );
        assert_eq!(time, trickle);
    }
    let (result, time) = runtime
        .request(&mut bucket, now + 20_000_000_000, 20000.0, 10000, 0)
        .unwrap();
    assert_eq!(result.tokens, 20000.0);
    assert_eq!(time, 10000);
}
#[test]
fn mcs_090_unlimited_nonfinite_and_zero_fill_override() {
    let mut bucket = configured_bucket(5000.0, 0, -1);
    let mut runtime = BucketRuntime::default();
    let grant = runtime.request(&mut bucket, 1, 123.0, 5000, 7).unwrap();
    assert_eq!(grant.0.tokens, 123.0);
    assert_eq!(grant.1, 0);
    assert_eq!(bucket.settings.as_ref().unwrap().fill_rate, 10000);
    assert!(runtime.request(&mut bucket, 2, f64::NAN, 5000, 7).is_none());
    assert_eq!(bucket.tokens, 0.0);
    let mut bucket = configured_bucket(10000.0, 2000, 200000);
    let mut runtime = BucketRuntime {
        override_fill_rate: Some(0.0),
        override_burst_limit: Some(0),
        ..Default::default()
    };
    for (now, client) in [(0, 1), (0, 2), (5_000_000_000, 1), (5_000_000_000, 2)] {
        let (result, time) = runtime
            .request(&mut bucket, now, 1000.0, 5000, client)
            .unwrap();
        assert_eq!(result.tokens, 0.0);
        assert_eq!(time, 5000);
    }
    assert!(
        runtime
            .slots
            .values()
            .all(|s| s.capacity == 0.0 && s.capacity.is_finite())
    );
}
#[test]
fn mcs_091_client_shares_expiration_and_zero_request() {
    let mut bucket = configured_bucket(0.0, 100, 100);
    let mut runtime = BucketRuntime::default();
    runtime.request(&mut bucket, 1, 1.0, 5000, 1).unwrap();
    let tracker = runtime.demands.get_mut(&1).unwrap();
    tracker.initialized = true;
    tracker.rate = 100.0;
    let (result, time) = runtime.request(&mut bucket, 1, 1.0, 5000, 2).unwrap();
    assert_eq!(result.tokens, 1.0);
    assert_eq!(time, 5000);
    assert_eq!(runtime.slots[&2].fill_rate, 50.0);
    runtime.request(&mut bucket, 2, 0.0, 5000, 2).unwrap();
    assert!(!runtime.slots.contains_key(&2));
    runtime
        .request(&mut bucket, 600_000_000_002, 1.0, 5000, 3)
        .unwrap();
    assert_eq!(runtime.slots.len(), 1);
    assert!(runtime.slots.contains_key(&3));
}
