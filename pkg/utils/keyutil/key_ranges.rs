// Copyright 2026 AsterSQL.
use crate::KeyRange;

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
