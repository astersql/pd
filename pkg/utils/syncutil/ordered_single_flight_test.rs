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
