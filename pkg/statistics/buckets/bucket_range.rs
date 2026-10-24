// Copyright 2026 AsterSQL.

use std::collections::HashMap;

use astersql_pd_keyutil::{max_key, min_key};
use astersql_pd_rangetree::{RangeItem, RangeTree};

use crate::{BucketStatus, BucketTreeItem};

impl RangeItem for BucketTreeItem {
    fn start_key(&self) -> &[u8] {
        &self.start_key
    }

    fn end_key(&self) -> &[u8] {
        &self.end_key
    }
}

impl BucketTreeItem {
    /// Copies and clips stats into an archived item with the supplied boundaries.
    ///
    /// Preserves Go's ordinary byte comparison for stat ends: an empty stat end
    /// can be excluded even though the item's range itself is unbounded.
    pub fn clone_by_range(&self, start: &[u8], end: &[u8]) -> Self {
        let stats = self
            .stats
            .iter()
            .filter_map(|stat| {
                let left = max_key(&stat.start_key, start);
                let right = if end.is_empty() {
                    &stat.end_key
                } else {
                    min_key(&stat.end_key, end)
                };
                (left < right).then(|| {
                    let mut copy = stat.clone();
                    copy.start_key = left.to_vec();
                    copy.end_key = right.to_vec();
                    copy
                })
            })
            .collect();
        Self {
            region_id: self.region_id,
            start_key: start.to_vec(),
            end_key: end.to_vec(),
            stats,
            interval: self.interval,
            version: self.version,
            status: BucketStatus::Archive,
        }
    }
}

/// Returns archived pieces left outside a covering range, in start-key order.
/// The range tree filters zero-width pieces, including the empty/empty sentinel.
pub fn bucket_debris_factory(
    start: &[u8],
    end: &[u8],
    item: &BucketTreeItem,
) -> Vec<BucketTreeItem> {
    let left = max_key(start, &item.start_key);
    let mut right = min_key(end, &item.end_key);
    if end.is_empty() {
        right = &item.end_key;
    }
    if item.end_key.is_empty() {
        right = end;
    }
    if left > right && !right.is_empty() {
        return Vec::new();
    }
    let mut debris = Vec::with_capacity(2);
    if item.start_key != left {
        debris.push(item.clone_by_range(&item.start_key, left));
    }
    if item.end_key != right || right.is_empty() {
        let start = if right.is_empty() {
            &item.end_key
        } else {
            right
        };
        debris.push(item.clone_by_range(start, &item.end_key));
    }
    debris
}

/// Range history and the current report per region, updated under one borrow.
/// Archived fragments remain in the tree for inheritance, outside the region map.
/// Returned query values are owned snapshots; callers supply any cross-thread lock.
pub struct BucketRangeIndex {
    tree: RangeTree<BucketTreeItem>,
    regions: HashMap<u64, BucketTreeItem>,
}

impl Default for BucketRangeIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl BucketRangeIndex {
    /// Creates an empty index with the canonical range tree and debris factory.
    pub fn new() -> Self {
        Self {
            tree: RangeTree::with_factory(10, bucket_debris_factory),
            regions: HashMap::new(),
        }
    }

    /// Installs a report, preserving archived fragments of overlapping reports.
    /// Overlaps are computed internally so callers cannot supply a stale list.
    pub fn put_item(&mut self, item: BucketTreeItem) {
        if self
            .regions
            .get(&item.region_id)
            .is_some_and(|old| old.start_key == item.start_key && old.end_key == item.end_key)
        {
            // Go overwrites the shared pointed-to object in place. Replace the
            // tree's owned value too, so both views observe all updated fields.
            self.tree.remove(&item);
            self.tree.update(item.clone());
            self.regions.insert(item.region_id, item);
            return;
        }
        for overlap in self.tree.update(item.clone()) {
            if overlap.status == BucketStatus::Alive {
                self.regions.remove(&overlap.region_id);
            }
        }
        self.regions.insert(item.region_id, item);
    }

    /// Returns ordered overlapping snapshots in the half-open query range.
    /// An empty end denotes positive infinity, matching the canonical tree.
    pub fn get_buckets_by_key_range(&self, start: &[u8], end: &[u8]) -> Vec<BucketTreeItem> {
        self.tree.get_overlaps(&BucketTreeItem {
            start_key: start.to_vec(),
            end_key: end.to_vec(),
            ..BucketTreeItem::default()
        })
    }

    /// Returns the current report for a region, excluding archived fragments.
    pub fn region(&self, id: u64) -> Option<&BucketTreeItem> {
        self.regions.get(&id)
    }

    pub(crate) fn current_regions(&self) -> impl Iterator<Item = &BucketTreeItem> {
        self.regions.values()
    }

    /// Returns the number of currently indexed region reports.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Returns the number of tree entries, including archived fragments.
    pub fn len(&self) -> usize {
        self.tree.len()
    }

    /// Reports whether there are no tree entries.
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
}
