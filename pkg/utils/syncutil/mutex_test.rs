// Copyright 2026 AsterSQL.

use std::sync::Arc;

use crate::{asynchronous, blocking};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutex_boundary() {
    let blocking_mutex = Arc::new(blocking::Mutex::new(0_u64));
    let panicking_mutex = Arc::clone(&blocking_mutex);
    let panic = std::thread::spawn(move || {
        let mut value = panicking_mutex.lock().expect("lock starts healthy");
        *value = 1;
        panic!("poison blocking mutex");
    })
    .join();
    assert!(panic.is_err());

    let poisoned = blocking_mutex
        .lock()
        .expect_err("std mutex exposes poisoning");
    assert_eq!(*poisoned.into_inner(), 1);

    let blocking_rwlock = blocking::RwLock::new(1_u64);
    *blocking_rwlock.write().expect("write lock is healthy") = 2;
    assert_eq!(*blocking_rwlock.read().expect("read lock is healthy"), 2);

    let asynchronous_mutex = Arc::new(asynchronous::Mutex::new(0_u64));
    let panicking_mutex = Arc::clone(&asynchronous_mutex);
    let panic = tokio::spawn(async move {
        let mut value = panicking_mutex.lock().await;
        *value = 1;
        panic!("drop async mutex guard");
    })
    .await
    .expect_err("task should panic");
    assert!(panic.is_panic());

    let mut value = asynchronous_mutex.lock().await;
    assert_eq!(*value, 1);
    *value = 2;
    drop(value);

    let asynchronous_rwlock = asynchronous::RwLock::new(1_u64);
    *asynchronous_rwlock.write().await = 2;
    assert_eq!(*asynchronous_rwlock.read().await, 2);
}
