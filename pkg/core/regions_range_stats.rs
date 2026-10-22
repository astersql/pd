// Copyright 2026 AsterSQL.
//! Range aggregates and contiguous neighbors over the canonical indexes.
use crate::{RegionInfo, RegionRole, RegionsInfo};
use std::sync::Arc;

impl RegionsInfo {
    /// Sums whole approximate sizes of intersecting regions, including partial
    /// intersections. Empty bounds select the complete index.
    pub fn get_region_size_by_range(&self, start: &[u8], end: &[u8]) -> i64 {
        if start.is_empty() && end.is_empty() {
            return self.root.total_size();
        }
        let mut size = 0i64;
        self.root.scan_range(start, |region| {
            if !end.is_empty() && region.meta().key_range.start_key.as_slice() >= end {
                return false;
            }
            size = size.wrapping_add(region.stats.approximate_size);
            true
        });
        size
    }
    /// Returns Go's signed range-rank count, including reversed-bound results.
    pub fn get_region_count_by_range(&self, start: &[u8], end: &[u8]) -> isize {
        self.root.get_count_by_range(start, end)
    }
    /// Counts a store role using the same range-rank semantics as the root.
    pub fn store_role_count_by_range(
        &self,
        store: u64,
        role: RegionRole,
        start: &[u8],
        end: &[u8],
    ) -> isize {
        self.role_tree(role, store)
            .map_or(0, |tree| tree.get_count_by_range(start, end))
    }
    /// Sums leader, follower and learner range counts; witnesses and pending
    /// membership do not add duplicate peers.
    pub fn store_peer_count_by_range(&self, store: u64, start: &[u8], end: &[u8]) -> isize {
        [
            RegionRole::Leader,
            RegionRole::Follower,
            RegionRole::Learner,
        ]
        .iter()
        .map(|role| self.store_role_count_by_range(store, *role, start, end))
        .sum()
    }
    /// Returns only neighbors with touching endpoints, retaining payload identity.
    pub fn get_adjacent_regions(
        &self,
        region: &RegionInfo,
    ) -> (Option<Arc<RegionInfo>>, Option<Arc<RegionInfo>>) {
        let (prev, next) = self.root.get_adjacent_regions(region);
        (
            prev.filter(|p| p.meta().key_range.end_key == region.meta().key_range.start_key),
            next.filter(|n| region.meta().key_range.end_key == n.meta().key_range.start_key),
        )
    }
    /// Returns uncovered intervals as uppercase hexadecimal endpoints, with
    /// empty strings for unbounded endpoints. An empty index returns no holes.
    pub fn get_range_holes(&self) -> Vec<[String; 2]> {
        fn hex(key: &[u8]) -> String {
            key.iter().map(|byte| format!("{byte:02X}")).collect()
        }
        let mut holes = Vec::new();
        let mut last = Vec::new();
        self.root.scan_range(b"", |region| {
            let range = &region.meta().key_range;
            if last != range.start_key {
                holes.push([hex(&last), hex(&range.start_key)]);
            }
            last.clone_from(&range.end_key);
            true
        });
        if !last.is_empty() {
            holes.push([hex(&last), String::new()]);
        }
        holes
    }
}
