// Copyright 2026 AsterSQL.
//! Go-compatible region splitting helper for synthetic region sets.
use crate::RegionInfo;

/// Split each snapshot using the first byte of each bound, returning left then right.
/// Empty bounds supply 0 and 255. Each operand is halved separately, matching Go's
/// truncation even for odd or reversed bounds. IDs and versions wrap as unsigned Go
/// integers do; this helper does not allocate globally unique IDs or validate ranges.
///
/// # Panics
/// Panics if an input has no epoch, matching the Go helper's nil dereference.
pub fn split_regions(regions: &[RegionInfo]) -> Vec<RegionInfo> {
    let mut results = Vec::with_capacity(regions.len().saturating_mul(2));
    let count = regions.len() as u64;
    for region in regions {
        let range = &region.meta.key_range;
        let start = range.start_key.first().copied().unwrap_or(0);
        let end = range.end_key.first().copied().unwrap_or(255);
        let middle = start / 2 + end / 2;
        let mut left = region.clone_with([]);
        left.meta.id = region.meta.id.wrapping_add(count);
        left.meta.key_range.end_key = vec![middle];
        let epoch = left
            .meta
            .epoch
            .as_mut()
            .expect("split requires a region epoch");
        epoch.version = epoch.version.wrapping_add(1);
        let mut right = region.clone_with([]);
        right.meta.id = region.meta.id.wrapping_add(count.wrapping_mul(2));
        right.meta.key_range.start_key = vec![middle];
        let epoch = right
            .meta
            .epoch
            .as_mut()
            .expect("split requires a region epoch");
        epoch.version = epoch.version.wrapping_add(1);
        results.extend([left, right]);
    }
    results
}
