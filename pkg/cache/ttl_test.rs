// Copyright 2026 AsterSQL.
use crate::{TTLString, TTLUint64, TtlDuration};
use std::{sync::Arc, time::Duration};
use tokio::{runtime::Handle, time::advance};
use tokio_util::sync::CancellationToken;

fn ttl(seconds: i64) -> TtlDuration {
    TtlDuration::from_nanos(seconds * 1_000_000_000)
}

#[tokio::test(start_paused = true)]
async fn ttl_cache_lifecycle_expiration_boundaries() {
    let cache = TTLUint64::new(
        Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(100),
        ttl(10),
    );
    cache.put(1, "default");
    cache.put_with_ttl(2, "custom", ttl(20));
    cache.put_with_ttl(3, "past", ttl(-1));
    cache.put_with_ttl(4, "zero", ttl(0));
    assert_eq!(cache.get(&4), Some("zero"));
    assert!(!cache.get_all_id().contains(&4));
    cache.remove(&4);
    assert_eq!(cache.get(&3), None);
    assert_eq!(cache.len(), 3); // Expired entries remain until GC or removal.
    advance(Duration::from_secs(10)).await;
    assert_eq!(cache.get(&1), Some("default")); // Go Get uses Before.
    assert!(!cache.get_all_id().contains(&1)); // Enumeration uses After.
    assert_eq!(cache.pop(), Some((2, "custom")));
    assert_eq!(cache.pop(), None);
    advance(Duration::from_nanos(1)).await;
    assert!(!cache.exists(&1));
    cache.remove(&1);
    cache.remove(&99);
    assert_eq!(cache.len(), 1);
    cache.clear();
    assert!(cache.is_empty());
    cache.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn ttl_cache_lifecycle_update_and_revival() {
    let cache = TTLString::new(
        Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(100),
        ttl(10),
    );
    cache.put("a".into(), 1);
    cache.put_with_ttl("b".into(), 2, ttl(2));
    advance(Duration::from_secs(5)).await;
    cache.update_ttl(ttl(10)); // Equal default must not refresh either entry.
    assert_eq!(cache.get(&"b".into()), None);
    advance(Duration::from_secs(6)).await;
    assert_eq!(cache.get(&"a".into()), None);
    cache.update_ttl(ttl(20)); // Even expired stored entries are revived.
    assert_eq!(cache.get(&"a".into()), Some(1));
    assert_eq!(cache.get(&"b".into()), Some(2));
    cache.put("c".into(), 3);
    cache.put("c".into(), 4);
    assert_eq!(cache.len(), 3);
    advance(Duration::from_secs(20)).await;
    assert_eq!(cache.get(&"c".into()), Some(4));
    assert!(cache.get_all_id().is_empty());
    assert_eq!(cache.pop(), None);
    cache.update_ttl(ttl(-1));
    assert_eq!(cache.get(&"c".into()), None);
    cache.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn ttl_cache_lifecycle_gc_restarts_and_cancels() {
    let cancellation = CancellationToken::new();
    let cache = TTLUint64::new(
        Handle::current(),
        cancellation.clone(),
        Duration::from_secs(2),
        ttl(2),
    );
    let value = Arc::new(7);
    cache.put(1, value.clone());
    advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert_eq!(cache.len(), 1); // GC also uses strict Before.
    advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert!(cache.is_empty());
    assert_eq!(Arc::strong_count(&value), 1);
    cache.put(2, value.clone()); // Start another worker after empty exit.
    advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert!(cache.is_empty());
    cache.put(3, value.clone());
    cancellation.cancel();
    tokio::task::yield_now().await;
    advance(Duration::from_secs(100)).await;
    assert_eq!(cache.len(), 1); // Cancellation stops GC, not cache operations.
    assert_eq!(cache.get(&3), None);
    cache.shutdown().await; // Join a worker that already observed external cancellation.
    cache.shutdown().await;

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let cache = TTLUint64::new(Handle::current(), cancelled, Duration::from_secs(1), ttl(0));
    cache.put(1, 1);
    cache.shutdown().await; // Cancellation before the worker ever polls.
    assert_eq!(cache.get(&1), Some(1));
}

#[tokio::test(start_paused = true)]
async fn ttl_cache_lifecycle_drop_releases_values_and_preserves_parent() {
    let parent = CancellationToken::new();
    let value = Arc::new(7);
    let cache = TTLString::new(
        Handle::current(),
        parent.clone(),
        Duration::from_secs(1),
        ttl(10),
    );
    cache.put("key".into(), value.clone());
    tokio::task::yield_now().await;
    drop(cache);
    tokio::task::yield_now().await;
    assert_eq!(Arc::strong_count(&value), 1);
    assert!(!parent.is_cancelled());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_cache_lifecycle_concurrent_put_pop() {
    let cache = Arc::new(TTLUint64::new(
        Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(100),
        ttl(100),
    ));
    let barrier = Arc::new(tokio::sync::Barrier::new(9));
    let mut tasks = Vec::new();
    for id in 0..8 {
        let cache = cache.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            cache.put(id, id * 2);
            assert_eq!(cache.get(&id), Some(id * 2));
        }));
    }
    barrier.wait().await;
    for task in tasks {
        task.await.unwrap();
    }
    let mut popped = Vec::new();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let cache = cache.clone();
        tasks.push(tokio::spawn(async move { cache.pop().unwrap() }));
    }
    for task in tasks {
        popped.push(task.await.unwrap());
    }
    popped.sort_unstable();
    assert_eq!(popped, (0..8).map(|id| (id, id * 2)).collect::<Vec<_>>());
    cache.shutdown().await;
}
