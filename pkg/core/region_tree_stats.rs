// Copyright 2026 AsterSQL.
//! Incremental accounting, independent of Arc ownership.
use crate::{RegionInfo, RegionTree};
use std::sync::atomic::{AtomicI32, Ordering};
#[derive(Debug, Default)]
pub(crate) struct BusinessRef(AtomicI32);
impl Clone for BusinessRef {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl RegionInfo {
    /// Increments the root/subtree business reference count.
    pub fn inc_ref(&self) {
        self.business_ref.0.fetch_add(1, Ordering::SeqCst);
    }
    /// Decrements the business reference count, independently of Arc handles.
    pub fn dec_ref(&self) {
        self.business_ref.0.fetch_sub(1, Ordering::SeqCst);
    }
    /// Returns the business reference count.
    pub fn get_ref(&self) -> i32 {
        self.business_ref.0.load(Ordering::SeqCst)
    }
}
impl RegionTree {
    /// Creates an index that counts root/subtree business references.
    pub fn with_count_ref() -> Self {
        Self {
            count_ref: true,
            ..Self::new()
        }
    }
    pub(crate) fn add_stat(&mut self, region: &RegionInfo, sign: i64) {
        self.total_size = self
            .total_size
            .wrapping_add(region.stats.approximate_size.wrapping_mul(sign));
        let rates = region.write_rate();
        self.total_rates.0 += rates.0 * sign as f64;
        self.total_rates.1 += rates.1 * sign as f64;
        if !region.loaded_from_storage() {
            self.not_from_storage += sign;
        }
    }
    /// Transfers an in-place replacement's business reference; uncounted trees do nothing.
    pub fn update_ref(&self, origin: &RegionInfo, region: &RegionInfo) {
        if self.count_ref {
            origin.dec_ref();
            region.inc_ref();
        }
    }
    /// Adjusts accounting for direct payload replacement. The caller replaces the
    /// indexed payload separately; this method does not mutate index keys or entries.
    pub fn update_stat(&mut self, origin: &RegionInfo, region: &RegionInfo) {
        self.add_stat(region, 1);
        self.add_stat(origin, -1);
        self.update_ref(origin, region);
    }
    /// Returns the total approximate size, or zero for an empty index.
    pub fn total_size(&self) -> i64 {
        if self.tree.len() == 0 {
            0
        } else {
            self.total_size
        }
    }
    /// Returns aggregate write bytes and keys per second.
    pub fn total_write_rate(&self) -> (f64, f64) {
        if self.tree.len() == 0 {
            (0., 0.)
        } else {
            self.total_rates
        }
    }
    /// Returns the number of entries whose source is not storage.
    pub fn not_from_storage_regions_count(&self) -> i64 {
        self.not_from_storage
    }
}
