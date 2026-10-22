// Copyright 2026 AsterSQL.
use crate::{Item, Lru};

/// Fixed-size 2Q cache matching Go's recent, frequent and ghost state machine.
/// Access requires mutable ownership; use `Cache` for synchronized snapshots.
pub struct TwoQueue<V> {
    size: usize,
    recent_size: usize,
    recent: Lru<V>,
    frequent: Lru<V>,
    ghost: Lru<()>,
}

impl<V: Clone> TwoQueue<V> {
    /// Creates a cache with recent ratio 0.25 and ghost ratio 0.5.
    pub fn new(size: usize) -> Self {
        Self::with_ratios(size, 0.25, 0.5)
    }

    /// Creates a cache with caller-selected nonnegative, finite queue ratios.
    /// As in Go, sizes truncate and a zero ghost capacity is unbounded.
    pub fn with_ratios(size: usize, recent_ratio: f64, ghost_ratio: f64) -> Self {
        assert!(recent_ratio.is_finite() && recent_ratio >= 0.0);
        assert!(ghost_ratio.is_finite() && ghost_ratio >= 0.0);
        Self {
            size,
            recent_size: (size as f64 * recent_ratio) as usize,
            recent: Lru::new(size),
            frequent: Lru::new(size),
            ghost: Lru::new((size as f64 * ghost_ratio) as usize),
        }
    }

    /// Inserts a value, promoting repeated resident or ghost keys to frequent.
    pub fn put(&mut self, key: u64, value: V) {
        if self.frequent.peek(key).is_some() {
            self.frequent.put(key, value);
        } else if self.recent.peek(key).is_some() {
            self.recent.remove(key);
            self.frequent.put(key, value);
        } else {
            let ghost = self.ghost.peek(key).is_some();
            self.ensure_space(ghost);
            if ghost {
                self.ghost.remove(key);
                self.frequent.put(key, value);
            } else {
                self.recent.put(key, value);
            }
        }
    }

    fn ensure_space(&mut self, ghost: bool) {
        if self.len() < self.size {
            return;
        }
        let recent_len = self.recent.len();
        if recent_len > 0
            && (recent_len > self.recent_size || (recent_len == self.recent_size && !ghost))
        {
            let key = self.recent.oldest_key().unwrap();
            self.recent.remove(key);
            self.ghost.put(key, ());
        } else if let Some(key) = self.frequent.oldest_key() {
            self.frequent.remove(key);
        }
    }

    /// Retrieves a value, promoting recent hits and refreshing frequent hits.
    pub fn get(&mut self, key: u64) -> Option<&V> {
        if self.frequent.peek(key).is_none() {
            let value = self.recent.peek(key)?.clone();
            self.recent.remove(key);
            self.frequent.put(key, value);
        }
        self.frequent.get(key)
    }

    /// Reads a resident value without changing queue order.
    pub fn peek(&self, key: u64) -> Option<&V> {
        self.frequent.peek(key).or_else(|| self.recent.peek(key))
    }

    /// Removes a resident key or its ghost history.
    pub fn remove(&mut self, key: u64) {
        self.recent.remove(key);
        self.frequent.remove(key);
        self.ghost.remove(key);
    }

    /// Returns recent then frequent snapshots, most recently used first in each.
    pub fn elems(&self) -> Vec<Item<V>> {
        let mut items = self.recent.elems();
        items.extend(self.frequent.elems());
        items
    }

    /// Counts resident entries only.
    pub fn len(&self) -> usize {
        self.recent.len() + self.frequent.len()
    }

    /// Reports whether no values are resident.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
