// Copyright 2026 AsterSQL.
//! In-memory cache algorithms with caller-owned values.

mod fifo;
pub use fifo::Fifo;

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
    TwoQueue {
        size: usize,
        recent: Lru<V>,
        frequent: Lru<V>,
        ghost: Lru<()>,
    },
}

impl<V: Clone> Cache<V> {
    /// Constructs a cache with the selected policy and Go's default 2Q ratios.
    pub fn new(size: usize, kind: CacheType) -> Self {
        let policy = match kind {
            CacheType::Lru => Policy::Lru(Lru::new(size)),
            CacheType::TwoQueue => Policy::TwoQueue {
                size,
                recent: Lru::new(size),
                frequent: Lru::new(size),
                ghost: Lru::new(size / 2),
            },
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
            Policy::TwoQueue {
                size,
                recent,
                frequent,
                ghost,
            } => {
                if frequent.peek(key).is_some() {
                    frequent.put(key, value);
                    return;
                }
                if recent.peek(key).is_some() {
                    recent.remove(key);
                    frequent.put(key, value);
                    return;
                }
                let was_ghost = ghost.peek(key).is_some();
                if recent.len() + frequent.len() >= *size {
                    let target = *size / 4;
                    if !recent.is_empty()
                        && (recent.len() > target || (recent.len() == target && !was_ghost))
                    {
                        let oldest = recent.oldest_key().unwrap();
                        recent.remove(oldest);
                        ghost.put(oldest, ());
                    } else if let Some(oldest) = frequent.oldest_key() {
                        frequent.remove(oldest);
                    }
                }
                if was_ghost {
                    ghost.remove(key);
                    frequent.put(key, value);
                } else {
                    recent.put(key, value);
                }
            }
        }
    }

    /// Retrieves a snapshot and updates use order under an exclusive lock.
    pub fn get(&self, key: u64) -> Option<V> {
        match &mut *self.inner.write().unwrap() {
            Policy::Lru(cache) => cache.get(key).cloned(),
            Policy::TwoQueue {
                recent, frequent, ..
            } => {
                if let Some(value) = frequent.get(key) {
                    return Some(value.clone());
                }
                let value = recent.peek(key)?.clone();
                recent.remove(key);
                frequent.put(key, value.clone());
                Some(value)
            }
        }
    }

    /// Reads a snapshot without changing use order.
    pub fn peek(&self, key: u64) -> Option<V> {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.peek(key).cloned(),
            Policy::TwoQueue {
                recent, frequent, ..
            } => frequent.peek(key).or_else(|| recent.peek(key)).cloned(),
        }
    }

    /// Removes an entry, including any 2Q ghost history.
    pub fn remove(&self, key: u64) {
        match &mut *self.inner.write().unwrap() {
            Policy::Lru(cache) => cache.remove(key),
            Policy::TwoQueue {
                recent,
                frequent,
                ghost,
                ..
            } => {
                recent.remove(key);
                frequent.remove(key);
                ghost.remove(key);
            }
        }
    }

    /// Returns snapshots in recent-then-frequent order for 2Q, MRU first per queue.
    pub fn elems(&self) -> Vec<Item<V>> {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.elems(),
            Policy::TwoQueue {
                recent, frequent, ..
            } => {
                let mut items = recent.elems();
                items.extend(frequent.elems());
                items
            }
        }
    }

    /// Counts resident entries, excluding 2Q ghosts.
    pub fn len(&self) -> usize {
        match &*self.inner.read().unwrap() {
            Policy::Lru(cache) => cache.len(),
            Policy::TwoQueue {
                recent, frequent, ..
            } => recent.len() + frequent.len(),
        }
    }

    /// Reports whether there are no resident entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
