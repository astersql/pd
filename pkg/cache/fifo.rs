// Copyright 2026 AsterSQL.
//! FIFO entries retain insertion order, including duplicate keys.

use std::collections::VecDeque;
use std::sync::RwLock;

use crate::Item;

/// Thread-safe FIFO cache. Zero capacity means unlimited capacity.
/// Returned snapshots clone values; use `Arc<T>` to retain shared identity.
pub struct Fifo<V> {
    max_count: usize,
    items: RwLock<VecDeque<Item<V>>>,
}

impl<V> Fifo<V> {
    /// Creates an empty FIFO cache.
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            items: RwLock::new(VecDeque::new()),
        }
    }

    /// Appends an entry and evicts the oldest when capacity is exceeded.
    /// Repeated keys remain separate entries, matching the Go FIFO.
    pub fn put(&self, key: u64, value: V) {
        let mut items = self.items.write().unwrap();
        items.push_back(Item { key, value });
        if self.max_count != 0 && items.len() > self.max_count {
            items.pop_front();
        }
    }

    /// Removes the oldest entry. Panics when empty, matching Go's list.Remove(nil).
    pub fn remove(&self) {
        assert!(
            self.items.write().unwrap().pop_front().is_some(),
            "empty FIFO"
        );
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.items.read().unwrap().len()
    }

    /// Reports whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<V: Clone> Fifo<V> {
    /// Returns snapshots from oldest to newest.
    pub fn elems(&self) -> Vec<Item<V>> {
        self.items.read().unwrap().iter().cloned().collect()
    }

    /// Returns entries whose keys are strictly greater than `key`, in insertion order.
    /// Keys need not be sorted and duplicate entries are preserved.
    pub fn from_elems(&self, key: u64) -> Vec<Item<V>> {
        self.items
            .read()
            .unwrap()
            .iter()
            .filter(|item| item.key > key)
            .cloned()
            .collect()
    }
}
