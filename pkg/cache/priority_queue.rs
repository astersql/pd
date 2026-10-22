// Copyright 2026 AsterSQL.
//! Priority admission matching priority_queue.go at 42a0da684cf28f16a48c74c54575b711031f3c5d.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Supplies the stable ID used to deduplicate queue values.
pub trait PriorityQueueItem {
    fn id(&self) -> u64;
}

impl<T: PriorityQueueItem> PriorityQueueItem for Arc<T> {
    fn id(&self) -> u64 {
        (**self).id()
    }
}

/// A priority and caller-owned value; updates retain the original value.
pub struct Entry<V> {
    pub priority: i64,
    pub value: Arc<V>,
}

impl<V> Clone for Entry<V> {
    fn clone(&self) -> Self {
        Self {
            priority: self.priority,
            value: Arc::clone(&self.value),
        }
    }
}

/// Bounded priority queue, with smaller numbers admitted ahead of larger ones.
/// Equal priorities compare equal, as in Go: they replace the sorted entry
/// while retaining both IDs in the lookup map. This queue requires exclusive
/// access for mutation; callers provide synchronization if needed.
pub struct PriorityQueue<V> {
    items: HashMap<u64, Entry<V>>,
    // This subset requires only replacement, removal and ordered traversal;
    // BTreeMap preserves priority-only equality without an ID tie breaker.
    ordered: BTreeMap<i64, Entry<V>>,
    capacity: isize,
}

impl<V: PriorityQueueItem> PriorityQueue<V> {
    /// Creates a queue; nonpositive capacity rejects every initial insertion.
    pub fn new(capacity: isize) -> Self {
        Self {
            items: HashMap::new(),
            ordered: BTreeMap::new(),
            capacity,
        }
    }

    /// Inserts an ID or updates its priority, preserving its original value.
    /// At capacity, a new ID must strictly outrank the worst sorted entry.
    pub fn put(&mut self, priority: i64, value: V) -> bool {
        let id = value.id();
        let entry = if let Some(existing) = self.items.get(&id) {
            let mut entry = existing.clone();
            if entry.priority != priority {
                self.ordered.remove(&entry.priority);
                entry.priority = priority;
            }
            entry
        } else {
            if self.capacity <= 0 || self.len() >= self.capacity as usize {
                let Some((&worst, victim)) = self.ordered.last_key_value() else {
                    return false;
                };
                if worst <= priority {
                    return false;
                }
                let victim_id = victim.value.id();
                self.ordered.remove(&worst);
                self.items.remove(&victim_id);
            }
            Entry {
                priority,
                value: Arc::new(value),
            }
        };
        self.ordered.insert(priority, entry.clone());
        self.items.insert(id, entry);
        true
    }

    /// Finds an ID without changing queue order, including displaced tied IDs.
    pub fn get(&self, id: u64) -> Option<&Entry<V>> {
        self.items.get(&id)
    }

    /// Returns the highest precedence entry (the smallest numeric priority).
    pub fn peek(&self) -> Option<&Entry<V>> {
        self.ordered.first_key_value().map(|(_, entry)| entry)
    }

    /// Returns the lowest precedence entry (the largest numeric priority).
    pub fn tail(&self) -> Option<&Entry<V>> {
        self.ordered.last_key_value().map(|(_, entry)| entry)
    }

    /// Removes an ID and its priority's sorted entry, as in Go's priority-only tree.
    /// Missing IDs have no effect; tied IDs may remain in the lookup map.
    pub fn remove(&mut self, id: u64) {
        if let Some(entry) = self.items.remove(&id) {
            self.ordered.remove(&entry.priority);
        }
    }

    /// Counts sorted entries, matching Go even when priorities collide.
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    /// Reports whether the sorted queue is empty.
    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    /// Returns entry snapshots from highest to lowest precedence.
    pub fn elems(&self) -> Vec<Entry<V>> {
        self.ordered.values().cloned().collect()
    }
}
