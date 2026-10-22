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

#[tokio::test]
async fn concurrency_tokens() {
    use tokio_util::sync::CancellationToken;
    let limiter = ConcurrencyLimiter::new(1);
    let cancel = CancellationToken::new();
    let first = limiter.acquire_token(&cancel).await.unwrap();
    let duplicate = first.clone();
    let other = ConcurrencyLimiter::new(1);
    other.release_token(&first);
    assert_eq!(other.get_running_tasks_num(), 0);
    let mut second = Box::pin(limiter.acquire_token(&cancel));
    let mut third = Box::pin(limiter.acquire_token(&cancel));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(second.as_mut().poll(cx).is_pending()))
            .await
    );
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(third.as_mut().poll(cx).is_pending()))
            .await
    );
    assert_eq!(limiter.get_waiting_tasks_num(), 2);
    limiter.release_token(&first);
    // A queued predecessor cannot be bypassed by a later waiter.
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(third.as_mut().poll(cx).is_pending()))
            .await
    );
    let second = second.await.unwrap();
    limiter.release_token(&duplicate);
    assert_eq!(limiter.get_running_tasks_num(), 1);
    limiter.release_token(&second);
    let third = third.await.unwrap();
    limiter.release_token(&third);
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);

    let held = limiter.acquire_token(&cancel).await.unwrap();
    let cancelled = CancellationToken::new();
    let mut waiting = Box::pin(limiter.acquire_token(&cancelled));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(waiting.as_mut().poll(cx).is_pending()))
            .await
    );
    cancelled.cancel();
    assert!(waiting.await.is_err());
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    let mut dropped = Box::pin(limiter.acquire_token(&cancel));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(dropped.as_mut().poll(cx).is_pending()))
            .await
    );
    drop(dropped);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    limiter.release_token(&held);

    let zero = ConcurrencyLimiter::new(0);
    assert!(zero.acquire_token(&cancelled).await.is_err());
    assert_eq!(zero.get_running_tasks_num(), 0);
    assert_eq!(zero.get_waiting_tasks_num(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrency_tokens_parallel() {
    use tokio_util::sync::CancellationToken;
    let limiter = Arc::new(ConcurrencyLimiter::new(7));
    let barrier = Arc::new(tokio::sync::Barrier::new(101));
    let mut handles = Vec::new();
    for _ in 0..100 {
        let limiter = limiter.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            let token = limiter
                .acquire_token(&CancellationToken::new())
                .await
                .unwrap();
            assert!(limiter.get_running_tasks_num() <= 7);
            tokio::task::yield_now().await;
            limiter.release_token(&token);
            limiter.release_token(&token);
        }));
    }
    barrier.wait().await;
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
}

#[tokio::test]
async fn concurrency_tokens_cancel_head() {
    use tokio_util::sync::CancellationToken;
    let limiter = ConcurrencyLimiter::new(1);
    let cancel = CancellationToken::new();
    let token = limiter.acquire_token(&cancel).await.unwrap();
    let head_cancel = CancellationToken::new();
    let mut head = Box::pin(limiter.acquire_token(&head_cancel));
    let mut tail = Box::pin(limiter.acquire_token(&cancel));
    for future in [&mut head, &mut tail] {
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                future.as_mut().poll(cx).is_pending()
            ))
            .await
        );
    }
    limiter.release_token(&token);
    head_cancel.cancel();
    assert!(head.await.is_err());
    let token = tail.await.unwrap();
    assert_eq!(limiter.get_running_tasks_num(), 1);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    limiter.release_token(&token);
}
