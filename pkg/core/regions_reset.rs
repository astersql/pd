// Copyright 2026 AsterSQL.
//! Explicit root/subtree removal and cache generation reset.
use crate::{RegionInfo, RegionState, RegionTree};
impl RegionState {
    /// Removes root and ID entries; subtree removal is a separate operation.
    pub fn remove_region(&mut self, region: &RegionInfo) {
        self.root.remove(region);
        self.regions.remove(&region.meta().id);
    }
    /// Removes role and overlap entries, releasing the subtree business reference.
    pub fn remove_region_from_sub_tree(&mut self, region: &RegionInfo) {
        self.subtrees.remove(region);
    }
    /// Removes the current root payload and its subtree entries if the ID exists.
    pub fn remove_region_if_exist(&mut self, id: u64) {
        if let Some(region) = self.get_region(id) {
            self.remove_region(&region);
            self.remove_region_from_sub_tree(&region);
        }
    }
    /// Starts empty indexes. As in Go, retained objects from the old cache keep
    /// their logical counts; new payloads begin an independent cache generation.
    pub fn reset_region_cache(&mut self) {
        self.root = RegionTree::with_count_ref();
        self.regions.clear();
        self.subtrees = Default::default();
    }
}
