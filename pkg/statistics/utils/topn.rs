// Copyright 2026 AsterSQL.

use std::{collections::HashMap, sync::Arc};

/// An identified item ordered independently in each dimension.
///
/// Implementations must provide a strict weak order and keep IDs/comparison values
/// immutable while retained; replace an item with `put` to change its rank.
/// Tied values have no guaranteed order or boundary membership.
pub trait TopNItem {
    /// The stable identity of this item.
    fn id(&self) -> u64;
    /// Whether this item is strictly smaller in the given dimension.
    fn less(&self, dimension: usize, than: &Self) -> bool;
}

/// The N largest items per dimension, retaining all remaining items as well.
///
/// Mutation requires exclusive access; callers can wrap this value in a lock
/// when sharing it. Returned immutable `Arc` snapshots survive replacements.
/// This collection does not expire items; TTL is a separate policy.
pub struct TopN<T> {
    dimensions: Vec<SingleTopN<T>>,
}

impl<T: TopNItem> TopN<T> {
    /// Creates a collection. Panics if dimensions or N is zero.
    pub fn new(dimensions: usize, n: usize) -> Self {
        assert!(
            dimensions > 0 && n > 0,
            "invalid TopN dimensions or capacity"
        );
        Self {
            dimensions: (0..dimensions).map(|k| SingleTopN::new(k, n)).collect(),
        }
    }

    /// Returns the number of all retained items, including those below N.
    pub fn len(&self) -> usize {
        let first = &self.dimensions[0];
        first.top.items.len() + first.rest.items.len()
    }

    /// Whether the collection contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Finds an item by identity, including items outside all top sets.
    pub fn get(&self, id: u64) -> Option<Arc<T>> {
        let first = &self.dimensions[0];
        first.top.get(id).or_else(|| first.rest.get(id))
    }

    /// Returns all retained items in unspecified order.
    pub fn get_all(&self) -> Vec<Arc<T>> {
        let first = &self.dimensions[0];
        first
            .top
            .items
            .iter()
            .chain(&first.rest.items)
            .cloned()
            .collect()
    }

    /// Returns up to N largest items, unordered. Panics for an invalid dimension.
    pub fn get_all_top_n(&self, dimension: usize) -> Vec<Arc<T>> {
        self.dimensions[dimension].top.items.clone()
    }

    /// Returns the smallest selected item, or None when empty.
    /// Panics for an invalid dimension.
    pub fn get_top_n_min(&self, dimension: usize) -> Option<Arc<T>> {
        self.dimensions[dimension].top.items.first().cloned()
    }

    /// Inserts or replaces an item across every dimension; true means replaced.
    pub fn put(&mut self, item: T) -> bool {
        let item = Arc::new(item);
        let mut updated = false;
        for dimension in &mut self.dimensions {
            updated = if dimension.top.index.contains_key(&item.id()) {
                dimension.top.put(Arc::clone(&item))
            } else {
                dimension.rest.put(Arc::clone(&item))
            };
            dimension.maintain();
        }
        updated
    }

    /// Removes an identity everywhere, returning its latest item if present.
    pub fn remove(&mut self, id: u64) -> Option<Arc<T>> {
        let mut removed = None;
        for dimension in &mut self.dimensions {
            removed = dimension
                .top
                .remove(id)
                .or_else(|| dimension.rest.remove(id));
            dimension.maintain();
        }
        removed
    }
}

struct SingleTopN<T> {
    n: usize,
    top: IndexedHeap<T>,
    rest: IndexedHeap<T>,
}

impl<T: TopNItem> SingleTopN<T> {
    fn new(dimension: usize, n: usize) -> Self {
        Self {
            n,
            top: IndexedHeap::new(dimension, false),
            rest: IndexedHeap::new(dimension, true),
        }
    }

    fn maintain(&mut self) {
        while self.top.items.len() < self.n && !self.rest.items.is_empty() {
            let promoted = self.rest.pop().unwrap();
            self.top.put(promoted);
        }
        while let (Some(top), Some(rest)) = (self.top.items.first(), self.rest.items.first()) {
            if !top.less(self.top.dimension, rest) {
                break;
            }
            let demoted = self.top.pop().unwrap();
            let promoted = self.rest.pop().unwrap();
            self.top.put(promoted);
            self.rest.put(demoted);
        }
    }
}

// A min heap for selected items, a max heap for the remainder. Both support
// arbitrary replacement/removal in O(log M) without scanning the full set.
struct IndexedHeap<T> {
    dimension: usize,
    reverse: bool,
    items: Vec<Arc<T>>,
    index: HashMap<u64, usize>,
}

impl<T: TopNItem> IndexedHeap<T> {
    fn new(dimension: usize, reverse: bool) -> Self {
        Self {
            dimension,
            reverse,
            items: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn less(&self, a: usize, b: usize) -> bool {
        if self.reverse {
            self.items[b].less(self.dimension, &self.items[a])
        } else {
            self.items[a].less(self.dimension, &self.items[b])
        }
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        self.index.insert(self.items[a].id(), a);
        self.index.insert(self.items[b].id(), b);
    }

    fn fix(&mut self, mut position: usize) {
        while position > 0 {
            let parent = (position - 1) / 2;
            if !self.less(position, parent) {
                break;
            }
            self.swap(position, parent);
            position = parent;
        }
        loop {
            let left = position * 2 + 1;
            if left >= self.items.len() {
                break;
            }
            let right = left + 1;
            let child = if right < self.items.len() && self.less(right, left) {
                right
            } else {
                left
            };
            if !self.less(child, position) {
                break;
            }
            self.swap(position, child);
            position = child;
        }
    }

    fn get(&self, id: u64) -> Option<Arc<T>> {
        self.index.get(&id).map(|&i| Arc::clone(&self.items[i]))
    }

    fn put(&mut self, item: Arc<T>) -> bool {
        let existing = self.index.get(&item.id()).copied();
        let position = existing.unwrap_or(self.items.len());
        if existing.is_some() {
            self.items[position] = item;
        } else {
            self.index.insert(item.id(), position);
            self.items.push(item);
        }
        self.fix(position);
        existing.is_some()
    }

    fn pop(&mut self) -> Option<Arc<T>> {
        let id = self.items.first()?.id();
        self.remove(id)
    }

    fn remove(&mut self, id: u64) -> Option<Arc<T>> {
        let position = self.index.remove(&id)?;
        let removed = self.items.swap_remove(position);
        if position < self.items.len() {
            self.index.insert(self.items[position].id(), position);
            self.fix(position);
        }
        Some(removed)
    }
}
