// Copyright 2026 AsterSQL.
use std::sync::Mutex;

use astersql_pd_keyutil::{KeyRange, KeyRanges};

/// A synchronized collection of scheduling key ranges.
#[derive(Debug, Default)]
pub struct Manager {
    ranges: Mutex<KeyRanges>,
}

impl Manager {
    /// Creates an empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reports whether no ranges have been appended.
    pub fn is_empty(&self) -> bool {
        self.ranges.lock().unwrap().is_empty()
    }

    /// Returns the portions of base not covered by stored ranges.
    pub fn get_non_overlapping_key_ranges(&self, base: &KeyRange) -> Vec<KeyRange> {
        self.ranges.lock().unwrap().subtract_key_ranges(base)
    }

    /// Deletes each supplied range from the stored collection in order.
    pub fn delete(&self, ranges: &[KeyRange]) {
        let mut stored = self.ranges.lock().unwrap();
        for range in ranges {
            stored.delete(range);
        }
    }

    /// Appends ranges and normalizes the collection under one lock.
    pub fn append(&self, ranges: &[KeyRange]) {
        let mut stored = self.ranges.lock().unwrap();
        for range in ranges {
            stored.append(&range.start_key, &range.end_key);
        }
        stored.sort_and_deduce();
    }
}

#[cfg(test)]
#[path = "range_append_test.rs"]
mod range_append_test;

#[cfg(test)]
#[path = "range_subtract_test.rs"]
mod range_subtract_test;
