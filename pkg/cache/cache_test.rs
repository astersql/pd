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
