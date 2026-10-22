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

// Queue age limits admission only: previously admitted tasks still execute.
#[tokio::test(start_paused = true)]
async fn runner_timeout_and_cancellation_age_boundary() {
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
    tokio::time::timeout(Duration::from_secs(5), async {
        while limiter.get_waiting_tasks_num() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runner.run_task(|_| async {}).unwrap(); // dispatch channel
    let (done, completed) = oneshot::channel();
    runner
        .run_task(move |_| async move {
            done.send(()).unwrap();
        })
        .unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    runner.run_task(|_| async {}).unwrap(); // equality is admitted
    tokio::time::advance(Duration::from_nanos(1)).await;
    let age_error = runner.run_task(|_| async {}).unwrap_err();
    assert_eq!(age_error, RunnerError::MaxWaitingTasksExceeded);
    assert_eq!(
        age_error.code(),
        Some("PD:ratelimit:ErrMaxWaitingTasksExceeded")
    );
    runner
        .run_task_with_id(1, "retained", true, |_| async {})
        .unwrap();
    let count_error = runner
        .run_task_with_id(2, "retained", true, |_| async {})
        .unwrap_err();
    assert_eq!(count_error.code(), age_error.code());
    assert_eq!(runner.pending_task_count(), 3);
    limiter.release_token(&held);
    tokio::time::timeout(Duration::from_secs(5), completed)
        .await
        .unwrap()
        .unwrap();
    runner.stop().await;
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
}

#[tokio::test(start_paused = true)]
async fn runner_timeout_and_cancellation_burst_cleanup() {
    use crate::RunnerError;
    use std::time::Duration;

    let parent = CancellationToken::new();
    let limiter = Arc::new(ConcurrencyLimiter::new(4));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    runner.start(&parent);
    let captured = Arc::new(());
    let weak = Arc::downgrade(&captured);
    let (entered, mut started) = tokio::sync::mpsc::unbounded_channel();
    let mut children = Vec::new();
    for _ in 0..4 {
        let entered = entered.clone();
        let captured = captured.clone();
        runner
            .run_task(move |cancel| async move {
                let _captured = captured;
                entered.send(cancel.clone()).unwrap();
                cancel.cancelled().await;
            })
            .unwrap();
        // Handshake fills all four permits without relying on scheduler timing.
        let child = tokio::time::timeout(Duration::from_secs(5), started.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!child.is_cancelled());
        children.push(child);
    }
    let queued_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..2048 {
        let queued_calls = queued_calls.clone();
        let captured = captured.clone();
        runner
            .run_task(move |_| async move {
                let _captured = captured;
                queued_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .unwrap();
    }
    drop(captured);
    tokio::time::timeout(Duration::from_secs(5), async {
        while limiter.get_waiting_tasks_num() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(runner.pending_task_count() > 0);
    parent.cancel();
    assert_eq!(runner.run_task(|_| async {}), Err(RunnerError::Stopped));
    assert_eq!(RunnerError::Stopped.code(), None);
    tokio::time::timeout(Duration::from_secs(5), runner.stop())
        .await
        .unwrap();
    assert!(children.iter().all(CancellationToken::is_cancelled));
    assert!(weak.upgrade().is_none());
    assert_eq!(queued_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(runner.pending_task_count(), 0);
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    runner.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn runner_timeout_and_cancellation_concurrent_burst_drains() {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    let limiter = Arc::new(ConcurrencyLimiter::new(4));
    let mut runner = ConcurrentRunner::new(Some(limiter.clone()));
    runner.start(&CancellationToken::new());
    let runner = Arc::new(runner);
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (done, mut completed) = tokio::sync::mpsc::unbounded_channel();
    let barrier = Arc::new(tokio::sync::Barrier::new(8));
    let mut producers = tokio::task::JoinSet::new();
    for producer in 0..8 {
        let runner = runner.clone();
        let active = active.clone();
        let peak = peak.clone();
        let done = done.clone();
        let barrier = barrier.clone();
        producers.spawn(async move {
            barrier.wait().await;
            for offset in 0..256 {
                let active = active.clone();
                let peak = peak.clone();
                let done = done.clone();
                runner
                    .run_task(move |_| async move {
                        let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(running, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        done.send(producer * 256 + offset).unwrap();
                    })
                    .unwrap();
            }
        });
    }
    drop(done);
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(result) = producers.join_next().await {
            result.unwrap();
        }
        let mut ids = Vec::new();
        while let Some(id) = completed.recv().await {
            ids.push(id);
        }
        ids.sort_unstable();
        assert_eq!(ids, (0..2048).collect::<Vec<_>>());
    })
    .await
    .unwrap();
    let mut runner = Arc::try_unwrap(runner).ok().unwrap();
    runner.stop().await;
    assert!((1..=4).contains(&peak.load(Ordering::SeqCst)));
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.get_running_tasks_num(), 0);
    assert_eq!(limiter.get_waiting_tasks_num(), 0);
    assert_eq!(runner.pending_task_count(), 0);
}
