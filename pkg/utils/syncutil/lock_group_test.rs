// Copyright 2026 AsterSQL.

use std::sync::Arc;

use crate::LockGroup;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lock_group_serialization() {
    let group = Arc::new(LockGroup::new());
    let first = group.lock(7).await;

    let same_id = group.lock(7);
    tokio::pin!(same_id);
    tokio::select! {
        biased;
        _ = &mut same_id => panic!("the same ID acquired the lock twice"),
        _ = tokio::task::yield_now() => {}
    }

    let different_id = group.lock(8);
    tokio::pin!(different_id);
    let second = tokio::select! {
        biased;
        guard = &mut different_id => guard,
        _ = tokio::task::yield_now() => panic!("different IDs blocked each other"),
    };
    drop(second);

    drop(first);
    drop(same_id.await);

    let colliding = Arc::new(LockGroup::with_hash(|_| 0));
    let first = colliding.lock(7).await;
    let same_hash = colliding.lock(8);
    tokio::pin!(same_hash);
    tokio::select! {
        biased;
        _ = &mut same_hash => panic!("colliding IDs did not share a lock"),
        _ = tokio::task::yield_now() => {}
    }
    drop(first);
    drop(same_hash.await);
}
