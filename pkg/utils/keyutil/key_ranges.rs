// Copyright 2026 AsterSQL.
use crate::{KeyRange, max_start_key, min_end_key};

/// An owned collection of key ranges; callers may supply unsorted ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyRanges {
    krs: Vec<KeyRange>,
}

impl KeyRanges {
    /// Creates a collection in the supplied order.
    pub fn new(ranges: Vec<KeyRange>) -> Self {
        Self { krs: ranges }
    }

    /// Creates an empty collection with space for the hinted number of ranges.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            krs: Vec::with_capacity(capacity),
        }
    }

    /// Drops all ranges while retaining the collection allocation for reuse.
    pub fn clean(&mut self) {
        self.krs.clear();
    }

    /// Reports whether the collection contains no ranges.
    pub fn is_empty(&self) -> bool {
        self.krs.is_empty()
    }

    /// Appends owned copies of byte boundaries in their supplied order.
    pub fn append(&mut self, start_key: impl AsRef<[u8]>, end_key: impl AsRef<[u8]>) {
        self.krs.push(KeyRange::new(start_key, end_key));
    }

    /// Borrows the ranges in their current order.
    pub fn ranges(&self) -> &[KeyRange] {
        &self.krs
    }

    /// Sorts by start bytes and joins adjacent ranges, matching Go SortAndDeduce.
    /// Non-adjacent overlaps and duplicates remain separate. Opposite empty
    /// boundaries also count as adjacent, as in KeyRange::is_adjacent.
    pub fn sort_and_deduce(&mut self) {
        self.krs.sort_by(|a, b| a.start_key.cmp(&b.start_key));
        self.join_adjacent(true);
    }

    /// Joins consecutive ranges whose end and next start bytes are equal.
    /// Preserves input order and leaves ordinary overlaps separate.
    pub fn merge(&mut self) {
        self.join_adjacent(false);
    }

    /// Returns the portions of `base` left by the supplied ranges in input order.
    /// Callers normally supply monotonically increasing ranges, as in Go.
    /// Empty bounds preserve Go's byte comparison and unbounded-end semantics;
    /// in particular, an entirely empty base with no ranges yields no slices.
    pub fn subtract_key_ranges(&self, base: &KeyRange) -> Vec<KeyRange> {
        let mut result = Vec::new();
        let mut start = base.start_key.as_slice();
        // Go's less(..., right) first compares bytes, then treats empty b as infinity.
        let before_end = |a: &[u8], b: &[u8]| a < b || (b.is_empty() && !a.is_empty());
        for range in &self.krs {
            if !base.over_lapped(range) {
                continue;
            }
            if start < range.start_key.as_slice() {
                result.push(KeyRange::new(
                    start,
                    min_end_key(&range.start_key, &base.end_key),
                ));
            }
            start = if start.is_empty() {
                &range.end_key
            } else {
                max_start_key(start, &range.end_key)
            };
            if !before_end(start, &base.end_key) {
                break;
            }
        }
        if before_end(start, &base.end_key) {
            result.push(KeyRange::new(start, &base.end_key));
        }
        result
    }

    /// Deletes `base` from every range, preserving input order and Go boundary semantics.
    /// Empty end bytes follow Go's lexical comparison before its infinity rule.
    pub fn delete(&mut self, base: &KeyRange) {
        let mut result = Vec::with_capacity(self.krs.len());
        for range in self.krs.drain(..) {
            if !range.over_lapped(base) {
                result.push(range);
                continue;
            }
            if range.start_key < base.start_key {
                result.push(KeyRange::new(
                    &range.start_key,
                    min_end_key(&range.end_key, &base.start_key),
                ));
            }
            if base.end_key < range.end_key
                || (range.end_key.is_empty() && !base.end_key.is_empty())
            {
                let start = if range.start_key.is_empty() {
                    &base.end_key
                } else {
                    max_start_key(&range.start_key, &base.end_key)
                };
                result.push(KeyRange::new(start, &range.end_key));
            }
        }
        self.krs = result;
    }

    fn join_adjacent(&mut self, symmetric: bool) {
        let mut merged: Vec<KeyRange> = Vec::with_capacity(self.krs.len());
        for range in self.krs.drain(..) {
            if let Some(last) = merged.last_mut()
                && (last.end_key == range.start_key || (symmetric && last.is_adjacent(&range)))
            {
                last.end_key = range.end_key;
            } else {
                merged.push(range);
            }
        }
        self.krs = merged;
    }
}
