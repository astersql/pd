// Copyright 2026 AsterSQL.
//! Validation and root commit share one exclusive registry borrow.
use crate::{RegionInfo, RegionsInfo, StaleRegionError};
use std::sync::Arc;

impl RegionsInfo {
    /// Checks the current root and commits without releasing exclusive access.
    /// Returns evicted regions in key order, excluding the same-ID origin.
    pub fn check_and_put_root_tree(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.pre_check_put_region(&region).2?;
        Ok(self.set_region(region).1)
    }

    /// Atomically validates and writes the root indexes.
    /// Store-role subtree maintenance is a separate domain operation.
    pub fn atomic_check_and_put_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.check_and_put_root_tree(region)
    }

    /// Returns evictions on success or the incoming region on stale rejection,
    /// matching the legacy Go entry point's deletion-candidate contract.
    pub fn check_and_put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.check_and_put_root_tree(region.clone())
            .unwrap_or_else(|_| vec![region])
    }
}
