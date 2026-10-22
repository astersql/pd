// Copyright 2026 AsterSQL.
//! LRU ordering follows lru.go: the front is most recently used.

use std::collections::HashMap;

/// A snapshot entry. Cloning a value such as `Arc<T>` preserves its shared identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<V> {
    pub key: u64,
    pub value: V,
}

struct Node<V> {
    item: Item<V>,
    previous: Option<usize>,
    next: Option<usize>,
}

/// A caller-synchronized least-recently-used cache with constant-time operations.
/// A capacity of zero means unlimited capacity, matching the Go implementation.
/// Mutating access requires `&mut self`; wrap the cache in a lock for shared use.
pub struct Lru<V> {
    max_count: usize,
    index: HashMap<u64, usize>,
    nodes: Vec<Option<Node<V>>>,
    free: Vec<usize>,
    front: Option<usize>,
    back: Option<usize>,
}

impl<V> Lru<V> {
    /// Creates an empty cache. Zero capacity disables eviction.
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            index: HashMap::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            front: None,
            back: None,
        }
    }

    /// Inserts or replaces a value and makes its key most recently used.
    pub fn put(&mut self, key: u64, value: V) {
        if let Some(&slot) = self.index.get(&key) {
            self.nodes[slot].as_mut().unwrap().item.value = value;
            self.promote(slot);
            return;
        }
        // Evict before allocating so bounded caches also bound arena storage.
        if self.max_count != 0 && self.len() == self.max_count {
            let oldest = self.nodes[self.back.unwrap()].as_ref().unwrap().item.key;
            self.remove(oldest);
        }
        let node = Node {
            item: Item { key, value },
            previous: None,
            next: None,
        };
        let slot = if let Some(slot) = self.free.pop() {
            self.nodes[slot] = Some(node);
            slot
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        };
        self.index.insert(key, slot);
        self.attach_front(slot);
    }

    /// Retrieves a value and makes its key most recently used.
    pub fn get(&mut self, key: u64) -> Option<&V> {
        let slot = *self.index.get(&key)?;
        self.promote(slot);
        Some(&self.nodes[slot].as_ref().unwrap().item.value)
    }

    /// Reads a value without changing recency.
    pub fn peek(&self, key: u64) -> Option<&V> {
        let slot = *self.index.get(&key)?;
        Some(&self.nodes[slot].as_ref().unwrap().item.value)
    }

    /// Removes a key, doing nothing when it is absent.
    pub fn remove(&mut self, key: u64) {
        if let Some(slot) = self.index.remove(&key) {
            self.detach(slot);
            self.nodes[slot] = None;
            self.free.push(slot);
        }
    }

    /// Returns independent entry snapshots from most to least recently used.
    pub fn elems(&self) -> Vec<Item<V>>
    where
        V: Clone,
    {
        let mut result = Vec::with_capacity(self.len());
        let mut cursor = self.front;
        while let Some(slot) = cursor {
            let node = self.nodes[slot].as_ref().unwrap();
            result.push(node.item.clone());
            cursor = node.next;
        }
        result
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Reports whether the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub(crate) fn oldest_key(&self) -> Option<u64> {
        self.back
            .map(|slot| self.nodes[slot].as_ref().unwrap().item.key)
    }

    fn promote(&mut self, slot: usize) {
        if self.front != Some(slot) {
            self.detach(slot);
            self.attach_front(slot);
        }
    }

    fn detach(&mut self, slot: usize) {
        let node = self.nodes[slot].as_ref().unwrap();
        let (previous, next) = (node.previous, node.next);
        if let Some(previous) = previous {
            self.nodes[previous].as_mut().unwrap().next = next;
        } else {
            self.front = next;
        }
        if let Some(next) = next {
            self.nodes[next].as_mut().unwrap().previous = previous;
        } else {
            self.back = previous;
        }
    }

    fn attach_front(&mut self, slot: usize) {
        let node = self.nodes[slot].as_mut().unwrap();
        node.previous = None;
        node.next = self.front;
        if let Some(front) = self.front {
            self.nodes[front].as_mut().unwrap().previous = Some(slot);
        } else {
            self.back = Some(slot);
        }
        self.front = Some(slot);
    }
}
