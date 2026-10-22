// Copyright 2026 AsterSQL.
//! Ordered scans and Go-compatible range rank accounting.
use crate::{RegionInfo, RegionItem, RegionTree};
use std::sync::Arc;

impl RegionTree {
    /// Visits the containing region, or the first later region, in start-key order.
    /// Returning false stops the scan after the current entry.
    pub fn scan_range(&self, start_key: &[u8], mut visit: impl FnMut(&Arc<RegionInfo>) -> bool) {
        let pivot = self
            .search(start_key)
            .map(RegionItem::new)
            .unwrap_or_else(|| RegionItem::pivot(start_key));
        self.tree
            .ascend_greater_or_equal(&pivot, |item| visit(item.region()));
    }

    /// Returns all indexed regions in start-key order, retaining shared identity.
    pub fn scan_ranges(&self) -> Vec<Arc<RegionInfo>> {
        let mut regions = Vec::with_capacity(self.tree.len());
        self.scan_range(b"", |region| {
            regions.push(region.clone());
            true
        });
        regions
    }

    /// Returns the end rank minus the adjusted start rank, matching Go GetCountByRange.
    /// An empty end includes the whole tail. Reversed bounds can yield negative
    /// counts. The predecessor adjustment compares end keys literally, so a
    /// predecessor with an empty (unbounded) end is not included by that adjustment.
    pub fn get_count_by_range(&self, start_key: &[u8], end_key: &[u8]) -> isize {
        let (exact, mut start) = self.tree.get_with_index(&RegionItem::pivot(start_key));
        if exact.is_none()
            && let Some(previous) = start
                .checked_sub(1)
                .and_then(|i| self.tree.get_at(i as isize))
            && previous.region().meta().key_range.end_key.as_slice() > start_key
        {
            start -= 1;
        }
        let end = if end_key.is_empty() {
            self.tree.len()
        } else {
            self.tree.get_with_index(&RegionItem::pivot(end_key)).1
        };
        end as isize - start as isize
    }
}
