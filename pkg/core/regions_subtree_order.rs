// Copyright 2026 AsterSQL.
//! Unordered subtree delivery with root refresh and metadata rejection.
use crate::{RegionInfo, RegionsInfo, check_region};
use std::sync::Arc;

impl RegionsInfo {
    /// Refreshes the event's ID from the root before applying it to subtrees.
    /// Missing IDs are ignored, including IDs already evicted by overlap updates.
    pub fn check_and_put_sub_tree(&mut self, event: &RegionInfo) {
        if let Some(current) = self.get_region(event.meta().id) {
            self.update_sub_tree_order_insensitive(current);
        }
    }

    /// Applies a possibly delayed event against the current subtree metadata.
    /// Equal epochs may replace statistics and membership in delivery order;
    /// a subsequent root refresh repairs that temporary divergence. Exclusive
    /// access is provided by the caller, as with the other registry operations.
    pub fn update_sub_tree_order_insensitive(&mut self, region: Arc<RegionInfo>) {
        let origin = self.subtrees.regions.get(&region.meta().id).cloned();
        if check_region(&region, origin.as_deref(), &[]).is_err() {
            return;
        }
        let range_changed = origin
            .as_ref()
            .is_none_or(|old| old.meta().key_range != region.meta().key_range);
        self.update_sub_tree(region, origin.as_deref(), &[], range_changed);
    }
}
