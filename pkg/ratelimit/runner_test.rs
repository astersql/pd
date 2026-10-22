// Copyright 2026 AsterSQL.
use crate::{ConcurrencyLimiter, ConcurrentRunner, SyncRunner};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn runner_lifecycle_dispatch_and_release() {
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    let parent = CancellationToken::new();
    runner.start(&parent);
    let (entered, wait) = oneshot::channel();
    runner
        .run_task(move |cancel| async move {
            entered.send(()).unwrap();
            cancel.cancelled().await
        })
        .unwrap();
    wait.await.unwrap();
    assert_eq!(limiter.get_running_tasks_num(), 1);
    let (done, completed) = oneshot::channel();
    runner
        .run_task(move |_| async move {
            let _ = done.send(());
        })
        .unwrap();
    runner.stop().await;
    assert!(completed.await.is_err());
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    assert!(!parent.is_cancelled());
    assert!(runner.run_task(|_| async {}).is_err());
    runner.stop().await;
}

#[tokio::test]
async fn runner_lifecycle_completion_schedules_next() {
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    runner.start(&CancellationToken::new());
    let mut receivers = Vec::new();
    for _ in 0..10 {
        let (tx, rx) = oneshot::channel();
        runner
            .run_task(move |_| async move {
                tx.send(()).unwrap();
            })
            .unwrap();
        receivers.push(rx);
    }
    for rx in receivers {
        rx.await.unwrap();
    }
    runner.stop().await;
    assert_eq!(limiter.get_running_tasks_num(), 0);
}

#[tokio::test]
async fn runner_lifecycle_panic_releases_token() {
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    runner.start(&CancellationToken::new());
    runner
        .run_task(|_| async { panic!("task failure") })
        .unwrap();
    let (tx, rx) = oneshot::channel();
    runner
        .run_task(move |_| async move {
            tx.send(()).unwrap();
        })
        .unwrap();
    rx.await.unwrap();
    runner.stop().await;
    assert_eq!(limiter.get_running_tasks_num(), 0);
}

#[tokio::test]
async fn runner_lifecycle_unlimited_and_parent_cancel() {
    let parent = CancellationToken::new();
    let mut runner = ConcurrentRunner::new(None);
    runner.start(&parent);
    let (tx, rx) = oneshot::channel();
    runner
        .run_task(move |cancel| async move {
            tx.send(()).unwrap();
            cancel.cancelled().await
        })
        .unwrap();
    rx.await.unwrap();
    parent.cancel();
    runner.stop().await;
}

#[test]
fn runner_lifecycle_sync() {
    let mut called = false;
    let runner = SyncRunner;
    runner.start(&CancellationToken::new());
    runner.run_task(|cancel| {
        assert!(!cancel.is_cancelled());
        called = true;
    });
    assert!(called);
    runner.stop();
}

#[tokio::test]
async fn runner_lifecycle_stop_during_acquire_and_restart() {
    let limiter = Arc::new(ConcurrencyLimiter::new(0));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    assert!(runner.run_task(|_| async {}).is_err());
    let parent = CancellationToken::new();
    runner.start(&parent);
    runner.start(&parent);
    let (tx, rx) = oneshot::channel();
    runner
        .run_task(move |_| async move {
            let _ = tx.send(());
        })
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while limiter.get_waiting_tasks_num() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runner.stop().await;
    assert!(rx.await.is_err());
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    assert_eq!(limiter.get_running_tasks_num(), 0);
    limiter.set_limit(1);
    runner.start(&parent);
    let (tx, rx) = oneshot::channel();
    runner
        .run_task(move |_| async move {
            tx.send(()).unwrap();
        })
        .unwrap();
    rx.await.unwrap();
    runner.stop().await;
    assert_eq!(limiter.get_running_tasks_num(), 0);
}
