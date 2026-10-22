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

#[tokio::test(start_paused = true)]
async fn runner_dedup_and_queue_age_retained_capacity() {
    use crate::RunnerError;
    use std::time::Duration;
    let limiter = Arc::new(ConcurrencyLimiter::new(1));
    let held = limiter
        .acquire_token(&CancellationToken::new())
        .await
        .unwrap();
    let mut runner =
        ConcurrentRunner::new(Some(limiter.clone())).with_queue_limits(Duration::from_secs(1), 3);
    runner.start(&CancellationToken::new());
    runner.run_task(|_| async {}).unwrap();
    while limiter.get_waiting_tasks_num() == 0 {
        tokio::task::yield_now().await;
    }
    // The dispatch channel holds one task; subsequent tasks stay pending.
    runner.run_task(|_| async {}).unwrap();
    let (old, obsolete) = oneshot::channel::<()>();
    runner
        .run_task_with_id(1, "a", false, move |_| async move {
            let _ = old.send(());
        })
        .unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    runner
        .run_task_with_id(1, "b", false, |_| async {})
        .unwrap();
    tokio::time::advance(Duration::from_nanos(1)).await;
    let (tx, rx) = oneshot::channel();
    runner
        .run_task_with_id(1, "a", true, move |_| async move {
            tx.send(()).unwrap();
        })
        .unwrap();
    assert!(obsolete.await.is_err());
    assert_eq!(runner.pending_task_count(), 2);
    assert_eq!(
        runner.run_task_with_id(2, "a", false, |_| async {}),
        Err(RunnerError::MaxWaitingTasksExceeded)
    );
    runner.run_task_with_id(2, "a", true, |_| async {}).unwrap();
    assert_eq!(
        runner.run_task_with_id(3, "a", true, |_| async {}),
        Err(RunnerError::MaxWaitingTasksExceeded)
    );
    assert_eq!(runner.pending_task_count(), 3);
    runner
        .run_task_with_id(2, "a", false, |_| async {})
        .unwrap();
    assert_eq!(runner.pending_task_count(), 3);
    assert_eq!(
        RunnerError::MaxWaitingTasksExceeded
            .error_template()
            .unwrap()
            .code(),
        "PD:ratelimit:ErrMaxWaitingTasksExceeded"
    );
    limiter.release_token(&held);
    tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .unwrap()
        .unwrap();
    runner.stop().await;
    assert_eq!(runner.pending_task_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn runner_dedup_and_queue_storage_reuse_release_fifo() {
    use super::{INITIAL_CAPACITY, TaskId};
    let mut runner = ConcurrentRunner::new(None);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    runner.sender = Some(sender.clone());
    runner
        .run_task_with_id(0, "queue", false, |_| async {})
        .unwrap();
    for id in 1..=INITIAL_CAPACITY {
        runner
            .run_task_with_id(id as u64, "queue", false, |_| async {})
            .unwrap();
    }
    let storage = runner
        .pending
        .lock()
        .unwrap()
        .order
        .iter()
        .find(|id| matches!(id, TaskId::Named(id, _) if *id == INITIAL_CAPACITY as u64))
        .unwrap() as *const TaskId;
    for _ in 0..INITIAL_CAPACITY / 2 {
        drop(receiver.try_recv().unwrap());
        runner.pending.lock().unwrap().dispatch(&sender);
    }
    for id in INITIAL_CAPACITY + 1..=INITIAL_CAPACITY + INITIAL_CAPACITY / 2 {
        runner
            .run_task_with_id(id as u64, "queue", false, |_| async {})
            .unwrap();
    }
    {
        let queue = runner.pending.lock().unwrap();
        assert_eq!(queue.order.capacity(), INITIAL_CAPACITY);
        // Wrapped storage remains in the same allocation.
        assert_eq!(
            queue
                .order
                .iter()
                .find(|id| matches!(id, TaskId::Named(id, _) if *id == INITIAL_CAPACITY as u64))
                .unwrap() as *const TaskId,
            storage
        );
        assert!(
            matches!(queue.order.front(), Some(TaskId::Named(id, _)) if *id == (INITIAL_CAPACITY/2+1) as u64)
        );
        assert!(
            matches!(queue.order.back(), Some(TaskId::Named(id, _)) if *id == (INITIAL_CAPACITY+INITIAL_CAPACITY/2) as u64)
        );
        assert_eq!(queue.entries.len(), INITIAL_CAPACITY);
    }
    for id in INITIAL_CAPACITY + INITIAL_CAPACITY / 2 + 1..=INITIAL_CAPACITY * 3 {
        runner
            .run_task_with_id(id as u64, "queue", false, |_| async {})
            .unwrap();
    }
    assert!(runner.pending.lock().unwrap().order.capacity() > INITIAL_CAPACITY);
    while runner.pending_task_count() > 0 {
        drop(receiver.try_recv().unwrap());
        runner.pending.lock().unwrap().dispatch(&sender);
    }
    drop(receiver.try_recv().unwrap());
    {
        let queue = runner.pending.lock().unwrap();
        assert_eq!(queue.order.capacity(), INITIAL_CAPACITY);
        assert!(queue.entries.is_empty());
    }
    // Cancel and join also release every captured object and queue allocation.
    runner.start(&CancellationToken::new());
    let captured = Arc::new(());
    let weak = Arc::downgrade(&captured);
    runner
        .run_task_with_id(1, "captured", false, move |_| async move {
            let _captured = captured;
            std::future::pending::<()>().await;
        })
        .unwrap();
    runner.stop().await;
    assert!(weak.upgrade().is_none());
    assert_eq!(runner.pending.lock().unwrap().order.capacity(), 0);
}

#[tokio::test]
async fn runner_dedup_and_queue_unlimited_dispatches_without_completion() {
    let mut runner = ConcurrentRunner::new(None);
    runner.start(&CancellationToken::new());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    for id in 0..10 {
        let tx = tx.clone();
        runner
            .run_task_with_id(id, "unlimited", false, move |cancel| async move {
                tx.send(id).unwrap();
                cancel.cancelled().await;
            })
            .unwrap();
    }
    let mut actual = Vec::new();
    for _ in 0..10 {
        actual.push(
            tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    actual.sort_unstable();
    assert_eq!(actual, (0..10).collect::<Vec<_>>());
    runner.stop().await;
}
