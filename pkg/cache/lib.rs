// Copyright 2026 AsterSQL.
//! In-memory cache algorithms with caller-owned values.

mod ttl;
pub use ttl::{TTLString, TTLUint64, TtlCache, TtlDuration};

mod fifo;
pub use fifo::Fifo;

mod priority_queue;
pub use priority_queue::{Entry, PriorityQueue, PriorityQueueItem};

mod two_queue;
pub use two_queue::TwoQueue;

mod lru;
pub use lru::{Item, Lru};

#[cfg(test)]
#[path = "cache_test.rs"]
mod cache_test;

use std::sync::RwLock;

/// Supported cache eviction policies. The default is LRU, as in cache.go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheType {
    #[default]
    Lru = 1,
    TwoQueue = 2,
}

/// Thread-safe cache with owned value snapshots (use `Arc<T>` for shared identity).
/// Get takes an exclusive lock because both policies mutate access order.
/// Peek, len and elems take shared locks. Lock poisoning is propagated as a panic.
pub struct Cache<V> {
    inner: RwLock<Policy<V>>,
}

enum Policy<V> {
    Lru(Lru<V>),
    TwoQueue(TwoQueue<V>),
}

impl<V: Clone> Cache<V> {
    /// Constructs a cache with the selected policy and Go's default 2Q ratios.
    pub fn new(size: usize, kind: CacheType) -> Self {
        let policy = match kind {
            CacheType::Lru => Policy::Lru(Lru::new(size)),
            CacheType::TwoQueue => Policy::TwoQueue(TwoQueue::new(size)),
        };
        Self {
            inner: RwLock::new(policy),
        }
    }

    /// Constructs a cache using the default LRU policy, without mutable globals.
    pub fn new_default(size: usize) -> Self {
        Self::new(size, CacheType::default())
    }

    /// Inserts or replaces an entry, updating its use order.
    pub fn put(&self, key: u64, value: V) {
        match &mut *self.inner.write().unwrap() {
            Policy::Lru(cache) => cache.put(key, value),
            Policy::TwoQueue(cache) => cache.put(key, value),
        }
    }

    /// Retrieves a snapshot and updates use order under an exclusive lock.
    pub fn get(&self, key: u64) -> Option<V> {
        match &mut *self.inner.write().unwrap() {
            Policy::Lru(cache) => cache.get(key).cloned(),
            Policy::TwoQueue(cache) => cache.get(key).cloned(),
        }
    }

    /// Reads a snapshot without changing use order.
    pub fn peek(&self, key: u64) -> Option<V> {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.peek(key).cloned(),
            Policy::TwoQueue(cache) => cache.peek(key).cloned(),
        }
    }

    /// Removes an entry, including any 2Q ghost history.
    pub fn remove(&self, key: u64) {
        match &mut *self.inner.write().unwrap() {
            Policy::Lru(cache) => cache.remove(key),
            Policy::TwoQueue(cache) => cache.remove(key),
        }
    }

    /// Returns snapshots in recent-then-frequent order for 2Q, MRU first per queue.
    pub fn elems(&self) -> Vec<Item<V>> {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.elems(),
            Policy::TwoQueue(cache) => cache.elems(),
        }
    }

    /// Counts resident entries, excluding 2Q ghosts.
    pub fn len(&self) -> usize {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.len(),
            Policy::TwoQueue(cache) => cache.len(),
        }
    }

    /// Reports whether there are no resident entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[path = "ttl_test.rs"]
mod ttl_test;
