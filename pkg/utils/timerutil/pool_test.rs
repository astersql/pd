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
