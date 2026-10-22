// Copyright 2026 AsterSQL.
use super::Lru;
use std::sync::Arc;

#[test]
fn lru_behavior() {
    let mut cache = Lru::new(3);
    assert!(cache.is_empty());
    assert_eq!(cache.get(99), None);
    assert_eq!(cache.peek(99), None);
    cache.remove(99);
    for key in 1..=3 {
        cache.put(key, key.to_string());
    }
    for key in [3, 2, 1] {
        assert_eq!(cache.get(key), Some(&key.to_string()));
    }
    cache.put(4, "4".into());
    assert_eq!(cache.len(), 3);
    assert_eq!(cache.get(3), None);
    for key in [1, 2, 4] {
        assert_eq!(cache.get(key), Some(&key.to_string()));
    }
    assert_eq!(cache.peek(1), Some(&"1".to_string()));
    assert_eq!(
        cache.elems().iter().map(|i| i.key).collect::<Vec<_>>(),
        [4, 2, 1]
    );
    let mut snapshot = cache.elems();
    snapshot[0].key = 99;
    snapshot[0].value = "changed".into();
    assert_eq!(cache.peek(4), Some(&"4".to_string()));
    cache.put(1, "updated".into());
    assert_eq!(cache.len(), 3);
    assert_eq!(
        cache.elems().iter().map(|i| i.key).collect::<Vec<_>>(),
        [1, 4, 2]
    );
    assert_eq!(cache.get(1), Some(&"updated".to_string()));
    cache.put(5, "5".into());
    assert_eq!(cache.peek(2), None);
    for key in [1, 4, 5, 99] {
        cache.remove(key);
    }
    assert!(cache.is_empty());
    cache.put(0, "reused".into());
    assert_eq!(cache.get(0), Some(&"reused".to_string()));
}

#[test]
fn lru_behavior_unbounded_and_shared_values() {
    let mut cache = Lru::new(0);
    let value = Arc::new(vec![42]);
    for key in 0..1000 {
        cache.put(key, Arc::clone(&value));
    }
    assert_eq!(cache.len(), 1000);
    assert!(Arc::ptr_eq(cache.get(0).unwrap(), &value));
    assert_eq!(cache.elems()[0].key, 0);
    let mut one = Lru::new(1);
    one.put(u64::MAX, 1);
    one.put(u64::MAX, 2);
    assert_eq!(one.get(u64::MAX), Some(&2));
    one.put(0, 3);
    assert_eq!(one.peek(u64::MAX), None);
    assert_eq!(one.len(), 1);
    one.remove(0);
    one.put(7, 4);
    assert_eq!(one.get(7), Some(&4));
}

#[test]
fn thread_safe_cache_factory() {
    use super::{Cache, CacheType};
    assert_eq!(CacheType::default(), CacheType::Lru);
    for kind in [CacheType::Lru, CacheType::TwoQueue] {
        let cache = Arc::new(Cache::new(3, kind));
        for key in 1..=3 {
            cache.put(key, Arc::new(key));
        }
        assert_eq!(cache.peek(1).as_deref(), Some(&1));
        assert_eq!(cache.elems()[0].key, 3);
        // Channels impose a happens-before order across real threads.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let first = Arc::clone(&cache);
            scope.spawn(move || {
                assert_eq!(first.get(3).as_deref(), Some(&3));
                tx.send(()).unwrap();
            });
            let second = Arc::clone(&cache);
            scope.spawn(move || {
                rx.recv().unwrap();
                assert_eq!(second.get(2).as_deref(), Some(&2));
                assert_eq!(second.get(1).as_deref(), Some(&1));
            });
        });
        cache.put(4, Arc::new(4));
        assert_eq!(cache.len(), 3);
        assert!(cache.peek(3).is_none());
        assert_eq!(cache.get(2).as_deref(), Some(&2));
        assert_eq!(cache.get(4).as_deref(), Some(&4));
        assert_eq!(
            cache.elems().iter().map(|i| i.key).collect::<Vec<_>>(),
            [4, 2, 1]
        );
        for key in [1, 2, 4, 99] {
            cache.remove(key);
        }
        assert!(cache.is_empty());
    }
    let cache = Cache::new_default(2);
    cache.put(1, "one");
    cache.put(2, "two");
    cache.get(1);
    cache.put(3, "three");
    assert_eq!(cache.peek(2), None);
    // Distinguish 2Q from LRU: a frequently used entry survives a scan.
    let cache = Cache::new(4, CacheType::TwoQueue);
    cache.put(1, 1);
    cache.get(1);
    for key in 2..=10 {
        cache.put(key, key);
    }
    assert_eq!(cache.peek(1), Some(1));
}

#[test]
fn thread_safe_cache_factory_concurrent_access() {
    use super::{Cache, CacheType};
    for kind in [CacheType::Lru, CacheType::TwoQueue] {
        let cache = Cache::new(64, kind);
        for key in 0..64 {
            cache.put(key, key);
        }
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let cache = &cache;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    for step in 0..512 {
                        let key = (worker + step) % 64;
                        assert_eq!(cache.get(key), Some(key));
                        assert_eq!(cache.peek(key), Some(key));
                        cache.put(key, key);
                        assert_eq!(cache.len(), 64);
                        let items = cache.elems();
                        assert_eq!(items.len(), 64);
                        let keys: std::collections::HashSet<_> =
                            items.iter().map(|item| item.key).collect();
                        assert_eq!(keys.len(), 64);
                        assert!(items.iter().all(|item| item.key == item.value));
                    }
                });
            }
        });
    }
}
