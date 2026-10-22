// Copyright 2026 AsterSQL.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tokio::sync::{Barrier, mpsc, oneshot};

use crate::OrderedSingleFlight;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn singleflight_coalesce_and_order() {
    const FOLLOWERS: usize = 5;

    let flight = Arc::new(OrderedSingleFlight::<usize, &'static str>::new());
    let executions = Arc::new(AtomicUsize::new(0));
    let (first_started_tx, first_started_rx) = oneshot::channel();
    let (first_finish_tx, first_finish_rx) = oneshot::channel();

    let first_flight = Arc::clone(&flight);
    let first_executions = Arc::clone(&executions);
    let first = tokio::spawn(async move {
        first_flight
            .call(move || async move {
                first_executions.fetch_add(1, Ordering::SeqCst);
                first_started_tx.send(()).unwrap();
                first_finish_rx.await.unwrap();
                Ok(101)
            })
            .await
    });
    first_started_rx.await.unwrap();

    let start = Arc::new(Barrier::new(FOLLOWERS + 1));
    let (registered_tx, mut registered_rx) = mpsc::channel(FOLLOWERS);
    let (second_finish_tx, second_finish_rx) = oneshot::channel();
    let second_finish = Arc::new(tokio::sync::Mutex::new(Some(second_finish_rx)));
    let mut followers = Vec::with_capacity(FOLLOWERS);

    for _ in 0..FOLLOWERS {
        let follower_flight = Arc::clone(&flight);
        let follower_executions = Arc::clone(&executions);
        let follower_start = Arc::clone(&start);
        let follower_registered = registered_tx.clone();
        let follower_finish = Arc::clone(&second_finish);
        followers.push(tokio::spawn(async move {
            follower_start.wait().await;
            let call = follower_flight.call(move || async move {
                follower_executions.fetch_add(1, Ordering::SeqCst);
                follower_finish.lock().await.take().unwrap().await.unwrap();
                Ok(102)
            });
            tokio::pin!(call);
            tokio::select! {
                biased;
                result = &mut call => panic!("follower completed before the first execution: {result:?}"),
                _ = tokio::task::yield_now() => {}
            }
            follower_registered.send(()).await.unwrap();
            call.await
        }));
    }
    drop(registered_tx);
    start.wait().await;
    for _ in 0..FOLLOWERS {
        registered_rx.recv().await.unwrap();
    }

    first_finish_tx.send(()).unwrap();
    assert_eq!(first.await.unwrap(), Ok(101));

    second_finish_tx.send(()).unwrap();
    for follower in followers {
        assert_eq!(follower.await.unwrap(), Ok(102));
    }
    assert_eq!(executions.load(Ordering::SeqCst), 2);
}

// Polling once registers a waiter without relying on scheduler timing.
async fn register<F: std::future::Future>(call: std::pin::Pin<&mut F>) {
    let mut call = call;
    std::future::poll_fn(|cx| {
        assert!(call.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn singleflight_cancellation_shared_waiters() {
    for cancel_executor in [false, true] {
        let flight = OrderedSingleFlight::<usize, ()>::new();
        let (release_tx, release_rx) = oneshot::channel();
        let first = flight.call(|| async {
            release_rx.await.unwrap();
            Ok(1)
        });
        tokio::pin!(first);
        register(first.as_mut()).await;

        let (started_tx, started_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();
        let mut second = Box::pin(flight.call_cancellable(move |cancel| async move {
            started_tx.send(cancel.clone()).unwrap();
            finish_rx.await.unwrap();
            assert!(!cancel.is_cancelled());
            Ok(2)
        }));
        register(second.as_mut()).await;
        let mut third = Box::pin(flight.call(|| async { Ok(99) }));
        register(third.as_mut()).await;
        release_tx.send(()).unwrap();
        assert_eq!(first.await, Ok(1));
        register(second.as_mut()).await;
        let cancel = started_rx.await.unwrap();
        if cancel_executor {
            drop(second);
            assert!(!cancel.is_cancelled());
            finish_tx.send(()).unwrap();
            assert_eq!(third.await, Ok(2));
        } else {
            drop(third);
            assert!(!cancel.is_cancelled());
            finish_tx.send(()).unwrap();
            assert_eq!(second.await, Ok(2));
        }
        cancel.cancelled().await;
    }
}

#[tokio::test]
async fn singleflight_cancellation_last_waiter() {
    let flight = OrderedSingleFlight::<usize, ()>::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (cancelled_tx, cancelled_rx) = oneshot::channel();
    let mut call = Box::pin(flight.call_cancellable(move |cancel| async move {
        started_tx.send(()).unwrap();
        cancel.cancelled().await;
        cancelled_tx.send(()).unwrap();
        Err(())
    }));
    register(call.as_mut()).await;
    started_rx.await.unwrap();
    drop(call);
    tokio::time::timeout(std::time::Duration::from_secs(5), cancelled_rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(flight.call(|| async { Ok(3) }).await, Ok(3));
}

#[tokio::test]
async fn singleflight_cancellation_pending_can_be_reused() {
    let flight = OrderedSingleFlight::<usize, ()>::new();
    let (release_tx, release_rx) = oneshot::channel();
    let first = flight.call(|| async {
        release_rx.await.unwrap();
        Ok(1)
    });
    tokio::pin!(first);
    register(first.as_mut()).await;
    let mut abandoned = Box::pin(flight.call(|| async { Ok(99) }));
    register(abandoned.as_mut()).await;
    drop(abandoned);
    let next = flight.call_cancellable(|cancel| async move {
        assert!(!cancel.is_cancelled());
        Ok(2)
    });
    tokio::pin!(next);
    register(next.as_mut()).await;
    release_tx.send(()).unwrap();
    assert_eq!(first.await, Ok(1));
    assert_eq!(next.await, Ok(2));
}

#[tokio::test]
async fn singleflight_cancellation_noncooperative_execution_keeps_order() {
    let flight = OrderedSingleFlight::<usize, ()>::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let mut first = Box::pin(flight.call(|| async {
        started_tx.send(()).unwrap();
        release_rx.await.unwrap();
        Ok(1)
    }));
    register(first.as_mut()).await;
    started_rx.await.unwrap();
    drop(first);
    let next = flight.call(|| async { Ok(2) });
    tokio::pin!(next);
    register(next.as_mut()).await;
    release_tx.send(()).unwrap();
    assert_eq!(next.await, Ok(2));
}
