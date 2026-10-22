// Copyright 2026 AsterSQL.

use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};

use crate::TimerPool;

async fn is_ready<F: Future + Unpin>(future: &mut F) -> bool {
    poll_fn(|context| Poll::Ready(Pin::new(&mut *future).poll(context).is_ready())).await
}

#[tokio::test(start_paused = true)]
async fn timer_checkout_and_expiry() {
    let pool = TimerPool::new();
    let mut first = pool.get(Duration::from_millis(20));
    let mut second = pool.get(Duration::from_millis(40));

    assert!(!is_ready(&mut first).await);
    assert!(!is_ready(&mut second).await);

    tokio::time::advance(Duration::from_millis(20)).await;
    assert!(is_ready(&mut first).await);
    assert!(!is_ready(&mut second).await);

    tokio::time::advance(Duration::from_millis(20)).await;
    assert!(is_ready(&mut second).await);
}

#[tokio::test(start_paused = true)]
async fn timer_reuse_and_cancel() {
    let pool = TimerPool::new();

    let mut expired = pool.get(Duration::from_millis(5));
    tokio::time::advance(Duration::from_millis(5)).await;
    assert!(is_ready(&mut expired).await);
    let storage_id = expired.storage_id();
    pool.put(expired);
    assert_eq!(pool.pooled_timer_count(), 1);

    let mut reused = pool.get(Duration::from_millis(50));
    assert_eq!(reused.storage_id(), storage_id);
    assert_eq!(pool.pooled_timer_count(), 0);
    assert!(!is_ready(&mut reused).await);
    tokio::time::advance(Duration::from_millis(5)).await;
    assert!(
        !is_ready(&mut reused).await,
        "stale expiry woke reused timer"
    );
    tokio::time::advance(Duration::from_millis(45)).await;
    assert!(is_ready(&mut reused).await);
    pool.put(reused);

    for _ in 0..100 {
        let mut cancelled = pool.get(Duration::from_secs(60));
        assert!(!is_ready(&mut cancelled).await);
        pool.put(cancelled);
    }
    assert_eq!(pool.pooled_timer_count(), 1);

    let mut after_cancellation = pool.get(Duration::from_millis(10));
    tokio::time::advance(Duration::from_millis(9)).await;
    assert!(!is_ready(&mut after_cancellation).await);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(is_ready(&mut after_cancellation).await);
}
