// Copyright 2026 AsterSQL.
//! Queries over non-overlapping half-open byte-key ranges.

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
}

impl<T: RangeItem> RangeTree<T> {
    /// Creates an empty tree. Panics when `degree` is less than two.
    pub fn new(degree: usize) -> Self {
        Self {
            tree: BTree::new(degree, T::less),
        }
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
