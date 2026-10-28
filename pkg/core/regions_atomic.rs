// Copyright 2026 AsterSQL.
//! Validation and root commit share one exclusive registry borrow.
use crate::{RegionInfo, RegionState, StaleRegionError};
use std::sync::Arc;

impl RegionState {
    /// Commit root and store-role indexes without a staleness check.
    pub fn put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        let (origin, overlaps, changed) = self.set_region(region.clone());
        self.update_sub_tree(region, origin.as_deref(), &overlaps, changed);
        overlaps
    }

    /// Checks the current root and commits without releasing exclusive access.
    /// Returns evicted regions in key order, excluding the same-ID origin.
    pub fn check_and_put_root_tree(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.pre_check_put_region(&region).2?;
        Ok(self.set_region(region).1)
    }

    /// Validate and commit both root and store-role indexes under exclusive access.
    pub fn atomic_check_and_put_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.pre_check_put_region(&region).2?;
        Ok(self.put_region(region))
    }

    /// Returns evictions on success or the incoming region on stale rejection,
    /// matching the legacy Go entry point's deletion-candidate contract.
    pub fn check_and_put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.atomic_check_and_put_region(region.clone())
            .unwrap_or_else(|_| vec![region])
    }
}
