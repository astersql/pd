// Copyright 2026 AsterSQL.
use super::*;
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, advance};
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn token_bucket_oracle_and_updates() {
    let l = RateLimiter::new(100.0, 100);
    assert!(l.available(1).await);
    assert!(l.allow_n(50).await);
    assert!(l.available(50).await);
    assert!(!l.available(100).await);
    assert!(l.available(50).await);
    assert!(l.allow_n(50).await);
    assert!(!l.available(50).await);
    advance(Duration::from_secs(1)).await;
    assert!(l.allow_n(99).await);
    assert!(l.allow().await);
    assert!(!l.available(1).await);
    advance(Duration::from_millis(500)).await;
    l.set_limit(10.0).await;
    assert_eq!(l.limit().await, 10.0);
    assert!(l.allow_n(50).await);
    l.set_burst(2).await;
    assert_eq!(l.burst().await, 2);
    assert!(!l.allow_n(3).await);
    advance(Duration::from_secs(1)).await;
    assert!(l.allow_n(2).await);
    assert!(!l.allow().await);
}

#[tokio::test(start_paused = true)]
async fn token_bucket_wait_cancel_deadline_and_refund() {
    let l = Arc::new(RateLimiter::new(2.0, 2));
    assert!(l.allow_n(2).await);
    let cancel = CancellationToken::new();
    assert_eq!(
        l.wait_n(3, &cancel, None).await,
        Err(WaitError::ExceedsBurst)
    );
    assert_eq!(
        l.wait_n(1, &cancel, Some(Instant::now())).await,
        Err(WaitError::Deadline)
    );
    let c = cancel.clone();
    let limiter = l.clone();
    let task = tokio::spawn(async move { limiter.wait_n(2, &c, None).await });
    tokio::task::yield_now().await;
    advance(Duration::from_millis(250)).await;
    cancel.cancel();
    assert_eq!(task.await.unwrap(), Err(WaitError::Cancelled));
    advance(Duration::from_millis(250)).await;
    assert!(l.allow().await); // cancellation refunded the reservation
    let c = CancellationToken::new();
    l.wait_n(1, &c, None).await.unwrap();
    assert!(!l.allow().await);
    c.cancel();
    assert_eq!(l.wait_n(0, &c, None).await, Err(WaitError::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn token_bucket_zero_infinite_and_concurrency() {
    let l = RateLimiter::new(0.0, 1);
    assert!(l.allow().await);
    assert!(!l.allow().await);
    assert!(l.allow_n(0).await);
    l.set_limit(f64::MAX).await;
    l.set_burst(0).await;
    assert!(l.allow_n(1000).await);
    let l = Arc::new(RateLimiter::new(0.0, 10));
    let mut jobs = Vec::new();
    for _ in 0..32 {
        let l = l.clone();
        jobs.push(tokio::spawn(async move { l.allow().await }));
    }
    let mut allowed = 0;
    for j in jobs {
        allowed += usize::from(j.await.unwrap());
    }
    assert_eq!(allowed, 10);
}

#[tokio::test(start_paused = true)]
async fn token_bucket_wait_serializes_and_drop_refunds() {
    let l = Arc::new(RateLimiter::new(1.0, 1));
    assert!(l.allow().await);
    let limiter = l.clone();
    let task =
        tokio::spawn(async move { limiter.wait_n(1, &CancellationToken::new(), None).await });
    tokio::task::yield_now().await;
    let limiter = l.clone();
    let update = tokio::spawn(async move { limiter.set_burst(2).await });
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    assert!(!update.is_finished());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    update.await.unwrap();
    advance(Duration::from_secs(1)).await;
    assert!(l.allow().await);
    assert!(!l.allow().await);
}

#[tokio::test(start_paused = true)]
async fn token_bucket_fractional_refill_and_burst_growth() {
    let l = RateLimiter::new(0.5, 1);
    assert!(l.allow().await);
    l.set_burst(3).await;
    assert!(!l.allow().await);
    advance(Duration::from_secs(1)).await;
    assert!(!l.available(1).await);
    assert!(!l.available(1).await);
    advance(Duration::from_secs(1)).await;
    assert!(l.available(1).await);
    assert!(l.allow().await);
    let l = RateLimiter::new(0.0, 1);
    assert!(l.allow().await);
    assert!(l.available(-1).await); // Go accepts signed counts and retains this credit
    assert!(l.allow().await);
}

#[tokio::test]
async fn token_bucket_available_does_not_consume_on_running_clock() {
    let l = RateLimiter::new(0.0, 1);
    for _ in 0..100 {
        assert!(l.available(1).await);
    }
    assert!(l.allow().await);
    assert!(!l.available(1).await);
}
