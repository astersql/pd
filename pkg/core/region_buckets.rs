// Copyright 2026 AsterSQL.
//! Bucket report updates on exclusively borrowed region snapshots.
use crate::{RegionBuckets, RegionInfo};
use std::sync::Arc;

impl RegionInfo {
    /// Replace reports only if `old` is the current snapshot by Arc identity.
    /// Clearing is unconditional. Like Go's test helper, this copies the candidate
    /// and does not impose version ordering. Clones have independent report slots.
    pub fn update_buckets(
        &mut self,
        buckets: Option<&RegionBuckets>,
        old: Option<&Arc<RegionBuckets>>,
    ) -> bool {
        let Some(buckets) = buckets else {
            self.report_buckets = None;
            return true;
        };
        let matches = match (self.report_buckets.as_ref(), old) {
            (None, None) => true,
            (Some(current), Some(old)) => Arc::ptr_eq(current, old),
            _ => false,
        };
        if matches {
            self.report_buckets = Some(Arc::new(buckets.clone()));
        }
        matches
    }

    /// Accept newer reports, retaining the candidate Arc; stale/equal reports
    /// count as success without replacement. An absent candidate has version zero.
    /// Rust's exclusive borrow serializes comparison and replacement; callers
    /// sharing a region across threads must hold their own synchronization guard.
    /// Unlike Go's optimistic CAS, this cannot fail from an intervening writer.
    pub fn compare_and_set_report_buckets(&mut self, buckets: Option<Arc<RegionBuckets>>) -> bool {
        if let Some(old) = &self.report_buckets {
            if buckets.as_ref().map_or(0, |b| b.version) <= old.version {
                return true;
            }
        }
        self.report_buckets = buckets;
        true
    }
}
