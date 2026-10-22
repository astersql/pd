// Copyright 2026 AsterSQL.
//! Random sampling with caller-owned randomness and Go-compatible retry semantics.
use crate::{KeyRange, RegionInfo, RegionItem, RegionTree};
use std::sync::Arc;

impl RegionTree {
    /// Samples one fully contained region. A rejected initial draw can return None
    /// even when another candidate would fit the requested range.
    pub fn random_region(
        &self,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Option<Arc<RegionInfo>> {
        self.random_regions(1, ranges, index, shuffle).pop()
    }

    /// Samples with replacement. Empty ranges select the whole tree. `index`
    /// must return a uniform value below its positive bound; `shuffle` must
    /// permute the supplied range indices. No global random state is installed.
    /// The first unsuccessful draw (or first unsuccessful multi-range pass)
    /// stops sampling. After the first success, sampling retries until `n` is
    /// reached, matching Go; a pathological random source can therefore stall.
    pub fn random_regions(
        &self,
        n: usize,
        ranges: &[KeyRange],
        mut index: impl FnMut(usize) -> usize,
        mut shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        if n == 0 || self.tree.len() == 0 {
            return vec![];
        }
        let whole = KeyRange::default();
        let ranges = if ranges.is_empty() {
            std::slice::from_ref(&whole)
        } else {
            ranges
        };
        let mut result = Vec::new();
        let mut order: Vec<_> = (0..ranges.len()).collect();
        while result.len() < n {
            if ranges.len() > 1 {
                shuffle(&mut order);
            }
            for &i in &order {
                let range = &ranges[i];
                let (exact, mut start) = self
                    .tree
                    .get_with_index(&RegionItem::pivot(&range.start_key));
                if exact.is_none()
                    && let Some(previous) = start
                        .checked_sub(1)
                        .and_then(|i| self.tree.get_at(i as isize))
                    && previous
                        .region()
                        .meta()
                        .key_range
                        .contains(&range.start_key)
                {
                    start -= 1;
                }
                let end = if range.end_key.is_empty() {
                    self.tree.len()
                } else {
                    self.tree
                        .get_with_index(&RegionItem::pivot(&range.end_key))
                        .1
                };
                if end <= start {
                    continue;
                }
                let offset = index(end - start);
                assert!(offset < end - start, "random index exceeds bound");
                let region = self
                    .tree
                    .get_at((start + offset) as isize)
                    .unwrap()
                    .region();
                let keys = &region.meta().key_range;
                if keys.start_key >= range.start_key
                    && (range.end_key.is_empty()
                        || (!keys.end_key.is_empty() && keys.end_key <= range.end_key))
                {
                    result.push(region.clone());
                    if result.len() == n {
                        return result;
                    }
                }
            }
            if result.is_empty() {
                break;
            }
        }
        result
    }
}
