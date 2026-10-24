// Copyright 2026 AsterSQL.
//! Hierarchical replica isolation and deferred region accounting.
use astersql_pd_core::{RegionInfo, StoreInfo};
use std::collections::{BTreeMap, BTreeSet};

/// Return the first level separating every store pair, or ("none", -1).
/// Missing/empty values match any value; keys and values use core's simple folding.
/// The returned name retains the exact spelling in `labels`.
pub fn get_region_label_isolation<'a>(
    stores: &[&StoreInfo],
    labels: &[&'a str],
) -> (&'a str, isize) {
    if stores.is_empty() || labels.is_empty() {
        return ("none", -1);
    }
    // Go's unresolved groups contain exactly the pairs that have not differed
    // at any preceding level. Pairwise comparison avoids duplicating groups
    // when missing values match multiple known values.
    let mut level = 0;
    for (i, store) in stores.iter().enumerate() {
        for other in &stores[i + 1..] {
            let Some(different) = store.compare_location(other, labels) else {
                return ("none", -1);
            };
            level = level.max(different);
        }
    }
    (labels[level], level as isize)
}

/// Whether the computed isolation is at or above the exact requested label key.
/// Empty and "none" requests always succeed; unknown keys fail.
pub fn is_region_label_isolation_satisfied(
    stores: &[&StoreInfo],
    labels: &[&str],
    isolation: &str,
) -> bool {
    if isolation.is_empty() || isolation == "none" {
        return true;
    }
    let (_, level) = get_region_label_isolation(stores, labels);
    level >= 0 && labels[level as usize..].contains(&isolation)
}

/// Owned label counts with deferred deletion. The host serializes mutation.
/// Snapshots include previously observed levels whose counts have fallen to zero.
#[derive(Default)]
pub struct LabelStatistics {
    regions: BTreeMap<u64, String>,
    counters: BTreeMap<String, i64>,
    defunct: BTreeSet<u64>,
}
impl LabelStatistics {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the region's isolation, counting each region exactly once.
    /// Re-observation does not cancel a previously marked deferred deletion.
    pub fn observe(&mut self, region: &RegionInfo, stores: &[&StoreInfo], labels: &[&str]) {
        let (isolation, _) = get_region_label_isolation(stores, labels);
        if let Some(old) = self.regions.get(&region.meta().id) {
            if old == isolation {
                return;
            }
            *self.counters.get_mut(old).expect("observed label counter") -= 1;
        }
        self.regions.insert(region.meta().id, isolation.to_owned());
        *self.counters.entry(isolation.to_owned()).or_default() += 1;
    }

    /// Mark an overlapping or removed region for the next cleanup.
    pub fn mark_defunct_region(&mut self, id: u64) {
        self.defunct.insert(id);
    }

    /// Remove each marked region once and drain all marks, including unknown IDs.
    pub fn clear_defunct_regions(&mut self) {
        for id in std::mem::take(&mut self.defunct) {
            if let Some(old) = self.regions.remove(&id) {
                *self.counters.get_mut(&old).expect("observed label counter") -= 1;
            }
        }
    }

    /// Independent count snapshot, suitable for a host-owned metrics collector.
    pub fn label_counter(&self) -> BTreeMap<String, i64> {
        self.counters.clone()
    }
}
