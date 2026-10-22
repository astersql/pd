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

#[test]
fn fifo_behavior() {
    use super::Fifo;
    let cache = Fifo::new(3);
    assert!(cache.is_empty());
    for key in 1..=4 {
        cache.put(key, key.to_string());
    }
    assert_eq!(cache.len(), 3);
    assert_eq!(
        cache.elems().iter().map(|i| i.key).collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(
        cache
            .from_elems(3)
            .iter()
            .map(|i| i.key)
            .collect::<Vec<_>>(),
        [4]
    );
    assert!(cache.from_elems(u64::MAX).is_empty());
    cache.remove();
    assert_eq!(cache.elems()[0].key, 3);
    cache.remove();
    cache.remove();
    assert!(cache.is_empty());
    let cache = Fifo::new(0);
    for key in [9, 2, 9, 0, u64::MAX] {
        cache.put(key, Arc::new(key));
    }
    assert_eq!(cache.len(), 5);
    assert_eq!(
        cache
            .from_elems(2)
            .iter()
            .map(|i| i.key)
            .collect::<Vec<_>>(),
        [9, 9, u64::MAX]
    );
    let mut snapshot = cache.elems();
    snapshot[0].key = 123;
    assert_eq!(cache.elems()[0].key, 9);
    assert!(Arc::ptr_eq(&snapshot[0].value, &cache.elems()[0].value));
    let one = Fifo::new(1);
    one.put(1, 1);
    one.put(1, 2);
    assert_eq!(one.elems()[0].value, 2);
    assert_eq!(one.len(), 1);
}

#[test]
#[should_panic(expected = "empty FIFO")]
fn fifo_behavior_empty_remove_panics() {
    super::Fifo::<u64>::new(0).remove();
}

#[test]
fn fifo_behavior_concurrent_insertion() {
    let cache = super::Fifo::new(0);
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let cache = &cache;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for step in 0..100 {
                    cache.put(worker * 100 + step, step);
                    assert!(!cache.elems().is_empty());
                }
            });
        }
    });
    assert_eq!(cache.len(), 400);
    let items = cache.elems();
    for worker in 0..4 {
        assert_eq!(
            items
                .iter()
                .filter(|item| item.key / 100 == worker)
                .map(|item| item.value)
                .collect::<Vec<_>>(),
            (0..100).collect::<Vec<_>>()
        );
    }
    for _ in 0..400 {
        cache.remove();
    }
    assert!(cache.is_empty());
}

#[test]
fn fifo_last_same_group() {
    let cache = super::Fifo::new(4);
    let group = || {
        cache.from_last_same_elems(|value: &Option<&str>| {
            (value.is_some(), value.unwrap_or_default().to_owned())
        })
    };
    assert!(group().is_empty());
    cache.put(1, Some("1"));
    cache.put(1, Some("2"));
    cache.put(1, Some("3"));
    assert_eq!(group().len(), 1);
    cache.put(1, Some("3"));
    cache.put(2, Some("3"));
    assert_eq!(group().iter().map(|i| i.key).collect::<Vec<_>>(), [2, 1, 1]);
    assert!(group().iter().all(|i| i.value == Some("3")));
    cache.put(1, Some("2"));
    assert_eq!(group().len(), 1);
    assert_eq!(group()[0].value, Some("2"));
    cache.put(3, None);
    assert_eq!(group().len(), 1);
    assert_eq!(group()[0].value, None);
    cache.put(4, Some("2"));
    assert_eq!(group().len(), 1);
    cache.put(5, Some("2"));
    assert_eq!(group().iter().map(|i| i.key).collect::<Vec<_>>(), [5, 4]);
    cache.put(6, Some("2"));
    cache.put(7, Some("2"));
    assert_eq!(
        group().iter().map(|i| i.key).collect::<Vec<_>>(),
        [7, 6, 5, 4]
    );
    assert_eq!(cache.len(), 4);
    assert_eq!(cache.elems()[0].key, 4);
    let single = super::Fifo::new(0);
    single.put(0, 10);
    assert_eq!(
        single
            .from_last_same_elems(|_| (false, String::new()))
            .len(),
        1
    );
}

#[test]
fn two_queue_behavior() {
    use super::TwoQueue;
    let mut c = TwoQueue::new(4);
    let keys = |c: &TwoQueue<u64>| c.elems().iter().map(|i| i.key).collect::<Vec<_>>();
    for k in 1..=4 {
        c.put(k, k);
    }
    assert_eq!(keys(&c), [4, 3, 2, 1]);
    assert_eq!(c.peek(1), Some(&1));
    assert_eq!(keys(&c), [4, 3, 2, 1]);
    assert_eq!(c.get(1), Some(&1));
    c.put(2, 20);
    assert_eq!(keys(&c), [4, 3, 2, 1]);
    c.put(5, 5); // recent 3 becomes a ghost
    assert_eq!(c.get(3), None);
    assert_eq!(c.len(), 4);
    c.put(3, 30); // ghost returns to frequent, recent 4 is evicted
    assert_eq!(keys(&c), [5, 3, 2, 1]);
    c.put(4, 40); // recent == target: ghost hit evicts frequent 1
    assert_eq!(keys(&c), [5, 4, 3, 2]);
    c.put(6, 6); // recent == target: new key evicts recent 5
    assert_eq!(keys(&c), [6, 4, 3, 2]);
    c.put(2, 200);
    assert_eq!(keys(&c), [6, 2, 4, 3]);
    assert_eq!(c.get(3), Some(&30));
    assert_eq!(keys(&c), [6, 3, 2, 4]);
    c.remove(5); // removing a ghost must prevent promotion on reinsertion
    c.put(5, 50);
    assert_eq!(keys(&c), [5, 3, 2, 4]);
    for k in [2, 3, 4, 5, 99] {
        c.remove(k);
    }
    assert!(c.is_empty());
    assert_eq!(c.get(99), None);
}

#[test]
fn two_queue_behavior_ghost_capacity_and_ratios() {
    use super::TwoQueue;
    let keys = |c: &TwoQueue<u64>| c.elems().iter().map(|i| i.key).collect::<Vec<_>>();
    let mut c = TwoQueue::with_ratios(4, 0.5, 0.25);
    for k in 1..=6 {
        c.put(k, k);
    }
    // Ghost capacity one discards key 1; key 2 remains a ghost.
    c.put(1, 10);
    assert_eq!(keys(&c), [1, 6, 5, 4]);
    c.put(3, 30);
    assert_eq!(keys(&c), [1, 6, 5, 3]);
    let mut unbounded_ghost = TwoQueue::with_ratios(4, 0.25, 0.0);
    for k in 1..=7 {
        unbounded_ghost.put(k, k);
    }
    unbounded_ghost.put(1, 10);
    assert_eq!(keys(&unbounded_ghost), [7, 6, 5, 1]);
    let mut small = TwoQueue::new(1);
    small.put(1, 1);
    small.put(2, 2);
    small.put(3, 3);
    // Go LRU treats ghost size zero as unbounded.
    small.put(1, 10);
    assert_eq!(keys(&small), [1]);
    assert_eq!(small.peek(1), Some(&10));
    let mut zero = TwoQueue::new(0);
    zero.put(1, 1);
    zero.put(2, 2);
    assert_eq!(keys(&zero), [2]);
    let mut shared = TwoQueue::new(2);
    let value = Arc::new(vec![1]);
    shared.put(1, Arc::clone(&value));
    assert!(Arc::ptr_eq(shared.get(1).unwrap(), &value));
}
