// Copyright 2026 AsterSQL.
use crate::{Controller, ControllerOption as O};
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn controller_labels_and_allowlist() {
    let parent = CancellationToken::new();
    let c = Controller::new(&parent);
    assert_eq!(c.update("a", &[]).await, 0);
    c.allow("missing").await.unwrap().done();
    assert_eq!(c.concurrency_status("missing").await, (0, 0));
    assert_eq!(c.qps_status("missing").await, (0.0, 0));
    assert_eq!(c.update("a", &[O::Concurrency(1)]).await, 4);
    assert_eq!(c.update("b", &[O::Concurrency(2)]).await, 4);
    let a = c.allow("a").await.unwrap();
    assert!(c.allow("a").await.is_err());
    let b = c.allow("b").await.unwrap();
    assert_eq!(c.concurrency_status("a").await, (1, 1));
    assert_eq!(c.concurrency_status("b").await, (2, 1));
    a.done();
    a.done();
    b.done();
    assert_eq!(c.update("a", &[O::Concurrency(1)]).await, 2);
    assert_eq!(c.update("a", &[O::Concurrency(0)]).await, 8);
    let old = c.allow("a").await.unwrap();
    assert_eq!(c.concurrency_status("a").await, (0, 1));
    old.done();
    assert!(!c.is_in_allow_list("free").await);
    assert_eq!(
        c.update("free", &[O::AllowList, O::Concurrency(1), O::Init])
            .await,
        16
    );
    assert!(c.is_in_allow_list("free").await);
    for _ in 0..10 {
        c.allow("free").await.unwrap().done();
    }
    assert_eq!(c.concurrency_status("free").await, (0, 0));
    assert_eq!(c.update("b", &[O::AllowList, O::Concurrency(99)]).await, 16);
    assert_eq!(c.concurrency_status("b").await, (2, 0));
    let b1 = c.allow("b").await.unwrap();
    let b2 = c.allow("b").await.unwrap();
    assert!(c.allow("b").await.is_err());
    b1.done();
    b2.done();
    assert_eq!(
        c.update("ordered", &[O::Concurrency(1), O::AllowList])
            .await,
        20
    );
    assert_eq!(c.update("init", &[O::Init, O::Concurrency(2)]).await, 6);
    c.close();
    c.close();
    assert!(c.is_closed());
    assert!(!parent.is_cancelled());
    assert_eq!(c.update("init", &[O::Concurrency(3)]).await, 4);
    c.allow("init").await.unwrap().done();
    let child = Controller::new(&parent);
    parent.cancel();
    assert!(child.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_labels_and_allowlist_parallel() {
    use std::sync::Arc;
    let c = Arc::new(Controller::new(&CancellationToken::new()));
    c.update("a", &[O::Concurrency(3)]).await;
    c.update("b", &[O::Concurrency(5)]).await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let c = c.clone();
        tasks.spawn(async move { (c.allow("a").await.ok(), c.allow("b").await.ok()) });
    }
    let mut handles = Vec::new();
    while let Some(result) = tasks.join_next().await {
        handles.push(result.unwrap());
    }
    assert_eq!(handles.iter().filter(|(a, _)| a.is_some()).count(), 3);
    assert_eq!(handles.iter().filter(|(_, b)| b.is_some()).count(), 5);
    assert_eq!(c.concurrency_status("a").await, (3, 3));
    assert_eq!(c.concurrency_status("b").await, (5, 5));
    for (a, b) in handles {
        for done in a.into_iter().chain(b) {
            done.done();
        }
    }
    assert_eq!(c.concurrency_status("a").await, (3, 0));
    assert_eq!(c.concurrency_status("b").await, (5, 0));
}
