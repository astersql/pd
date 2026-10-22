// Copyright 2026 AsterSQL.
//! Overlap-aware mutations of the start-key index.
use crate::{RegionInfo, RegionItem, RegionTree};
use std::sync::Arc;

impl RegionTree {
    /// Returns overlapping entries in start-key order, including a containing predecessor.
    pub fn overlaps(&self, region: &RegionInfo) -> Vec<Arc<RegionInfo>> {
        let range = &region.meta().key_range;
        let start = self.search(&range.start_key);
        let pivot = RegionItem::pivot(start.as_ref().map_or(range.start_key.as_slice(), |r| {
            r.meta().key_range.start_key.as_slice()
        }));
        let mut result = Vec::new();
        self.tree.ascend_greater_or_equal(&pivot, |item| {
            if !range.end_key.is_empty()
                && range.end_key <= item.region().meta().key_range.start_key
            {
                return false;
            }
            result.push(item.region().clone());
            true
        });
        result
    }

    /// Removes entire overlapping entries and inserts the region, returning old entries.
    /// IDs do not participate in ordering: callers moving an ID to a disjoint range
    /// must explicitly remove its old entry.
    pub fn update(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        let overlaps = self.overlaps(&region);
        self.add_stat(&region, 1);
        for old in &overlaps {
            self.add_stat(old, -1);
            if self.count_ref {
                old.dec_ref();
            }
            self.tree.delete(&RegionItem::new(old.clone()));
        }
        if self.count_ref {
            region.inc_ref();
        }
        self.tree.replace_or_insert(RegionItem::new(region));
        overlaps
    }

    /// Removes the entry at the supplied start key if its containing region has the
    /// same ID. The supplied end key does not participate in deletion.
    pub fn remove(&mut self, region: &RegionInfo) {
        let start = &region.meta().key_range.start_key;
        if let Some(old) = self
            .search(start)
            .filter(|old| old.meta().id == region.meta().id)
        {
            self.total_size = self.total_size.wrapping_sub(old.stats.approximate_size);
            let rates = old.write_rate();
            self.total_rates.0 -= rates.0;
            self.total_rates.1 -= rates.1;
            if !region.loaded_from_storage() {
                self.not_from_storage -= 1;
            }
            if self.count_ref {
                old.dec_ref();
            }
            self.tree.delete(&RegionItem::pivot(start));
        }
    }
}
