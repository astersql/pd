// Copyright 2026 AsterSQL.
//! Merge and split eligibility over normalized size (MiB) and key counts.
use crate::RegionInfo;

/// Whether approximate statistics represent an empty region, including size one.
pub fn is_empty_region(size: i64, keys: i64) -> bool {
    size <= 1 && keys == 0
}

impl RegionInfo {
    /// Merge when both approximate counters are at or below their limits.
    pub fn need_merge(&self, merge_size: i64, merge_keys: i64) -> bool {
        self.stats.approximate_size <= merge_size && self.stats.approximate_keys <= merge_keys
    }

    /// A region is oversized when either counter reaches its limit.
    pub fn is_oversized(&self, max_size: i64, max_keys: i64) -> bool {
        self.stats.approximate_size >= max_size || self.stats.approximate_keys >= max_keys
    }
}
