// Copyright 2026 AsterSQL.
//! Queries and updates over non-overlapping half-open byte-key ranges.

use astersql_pd_btree::BTree;

/// An item ordered by its start key, with an empty end key meaning infinity.
pub trait RangeItem: Clone {
    fn start_key(&self) -> &[u8];
    fn end_key(&self) -> &[u8];
    /// Strict weak ordering, consistent with start-key order.
    fn less(&self, other: &Self) -> bool {
        self.start_key() < other.start_key()
    }
}

/// A range index backed by the canonical order-statistics B-tree.
pub struct RangeTree<T> {
    tree: BTree<T, fn(&T, &T) -> bool>,
    factory: Box<DebrisFactory<T>>,
}

/// Produces remaining pieces of an old range when a new range covers it.
pub type DebrisFactory<T> = dyn FnMut(&[u8], &[u8], &T) -> Vec<T>;

impl<T: RangeItem> RangeTree<T> {
    /// Creates an empty tree whose updates discard overlapping ranges entirely.
    /// Panics when `degree` is less than two.
    pub fn new(degree: usize) -> Self {
        Self::with_factory(degree, |_, _, _| Vec::new())
    }

    /// Creates an empty tree with a factory for preserving old range fragments.
    /// The factory receives the new start/end and each overlapping old item.
    /// Panics when `degree` is less than two.
    pub fn with_factory(
        degree: usize,
        factory: impl FnMut(&[u8], &[u8], &T) -> Vec<T> + 'static,
    ) -> Self {
        Self {
            tree: BTree::new(degree, T::less),
            factory: Box::new(factory),
        }
    }

    /// Replaces overlaps, retaining valid factory fragments, then inserts `item`.
    /// Returns the original overlaps in ascending start-key order. Fragments
    /// must have start < end, or a nonempty start with an unbounded empty end.
    /// The factory is responsible for keeping fragments outside the new range.
    pub fn update(&mut self, item: T) -> Vec<T> {
        let overlaps = self.get_overlaps(&item);
        for old in &overlaps {
            self.tree.delete(old);
            for child in (self.factory)(item.start_key(), item.end_key(), old) {
                if child.start_key() < child.end_key()
                    || (!child.start_key().is_empty() && child.end_key().is_empty())
                {
                    self.tree.replace_or_insert(child);
                }
            }
        }
        self.tree.replace_or_insert(item);
        overlaps
    }

    /// Builds an index from already non-overlapping ranges.
    /// Comparator-equivalent starts replace earlier items, as in the Go B-tree.
    /// This constructor does not split overlapping ranges or produce debris.
    pub fn from_items(degree: usize, items: impl IntoIterator<Item = T>) -> Self {
        let mut result = Self::new(degree);
        for item in items {
            result.tree.replace_or_insert(item);
        }
        result
    }

    /// Returns the predecessor containing the query's start key in `[start, end)`.
    pub fn find(&self, item: &T) -> Option<&T> {
        let (exact, rank) = self.tree.get_with_index(item);
        let candidate = exact.or_else(|| {
            rank.checked_sub(1)
                .and_then(|i| self.tree.get_at(i as isize))
        })?;
        let key = item.start_key();
        (key >= candidate.start_key()
            && (candidate.end_key().is_empty() || key < candidate.end_key()))
        .then_some(candidate)
    }

    /// Returns overlaps in ascending start-key order, with an empty end unbounded.
    /// Scanning includes the range containing the query's start, if any, and
    /// stops before a range whose start is at or beyond the query's end.
    pub fn get_overlaps(&self, item: &T) -> Vec<T> {
        let pivot = self.find(item).unwrap_or(item);
        let end = item.end_key();
        let mut overlaps = Vec::new();
        self.tree.ascend_greater_or_equal(pivot, |candidate| {
            if !end.is_empty() && end <= candidate.start_key() {
                return false;
            }
            overlaps.push(candidate.clone());
            true
        });
        overlaps
    }

    /// Removes the comparator-equivalent item and returns its stored value.
    /// A start key merely contained in a range does not remove that range.
    pub fn remove(&mut self, item: &T) -> Option<T> {
        self.tree.delete(item)
    }

    /// Scans from the range containing `start`, or the first later range.
    /// Stops immediately when `visit` returns false; the query end is ignored.
    pub fn scan_range(&self, start: &T, visit: impl FnMut(&T) -> bool) {
        let pivot = self.find(start).unwrap_or(start);
        self.tree.ascend_greater_or_equal(pivot, visit);
    }

    /// Returns the previous and next items, skipping items with the query start.
    /// For an absent start key, returns its immediate surrounding items even
    /// when the query lies inside the previous range.
    pub fn get_adjacent_item(&self, item: &T) -> (Option<&T>, Option<&T>) {
        let (exact, rank) = self.tree.get_with_index(item);
        let prev = rank
            .checked_sub(1)
            .and_then(|i| self.tree.get_at(i as isize));
        let next_rank = rank + usize::from(exact.is_some());
        let next = self.tree.get_at(next_rank as isize);
        (prev, next)
    }

    /// Returns the number of indexed ranges.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Reports whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    /// Returns the range at zero-based rank, or `None` outside the index.
    pub fn get_at(&self, index: isize) -> Option<&T> {
        self.tree.get_at(index)
    }

    /// Returns the comparator-equivalent range and rank; an absent key returns
    /// `None` with its insertion rank, even when it lies inside a stored range.
    pub fn get_with_index(&self, item: &T) -> (Option<&T>, usize) {
        self.tree.get_with_index(item)
    }
}

#[cfg(test)]
#[path = "range_tree_test.rs"]
mod tests;
