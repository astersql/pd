// Copyright 2026 AsterSQL.
//! Ordered registry scans with Go-compatible bounds and limits.
use crate::{RegionInfo, RegionState};
use std::sync::Arc;

impl RegionState {
    /// Scans from the containing or next region, excluding starts at or beyond
    /// a nonempty end key. Nonpositive limits include all candidates across holes.
    /// Bounds follow Go's literal start-key comparison, including equal bounds
    /// inside a region. Returned Arcs retain the registry payload identity.
    pub fn scan_regions(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        limit: isize,
    ) -> Vec<Arc<RegionInfo>> {
        let mut result = Vec::new();
        self.root.scan_range(start_key, |region| {
            if (!end_key.is_empty() && region.meta().key_range.start_key.as_slice() >= end_key)
                || (limit > 0 && result.len() >= limit as usize)
            {
                return false;
            }
            result.push(region.clone());
            true
        });
        result
    }

    /// Visits the containing or next region and then each later region across
    /// holes. A false callback stops immediately after the current region.
    pub fn scan_region_with_iterator(
        &self,
        start_key: &[u8],
        iterator: impl FnMut(&Arc<RegionInfo>) -> bool,
    ) {
        self.root.scan_range(start_key, iterator);
    }
}
