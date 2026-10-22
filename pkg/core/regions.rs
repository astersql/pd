// Copyright 2026 AsterSQL.
//! ID and root-range indexes updated together through exclusive mutation.
use crate::{RegionInfo, RegionItem, RegionTree};
use std::collections::HashMap;
use std::sync::Arc;

/// Region registry. Borrowing keeps each read consistent with the root index;
/// shared concurrent owners may place the registry behind their own lock.
pub struct RegionsInfo {
    pub(crate) root: RegionTree,
    pub(crate) subtrees: crate::regions_subtree::SubTrees,
    regions: HashMap<u64, Arc<RegionInfo>>,
}
impl Default for RegionsInfo {
    fn default() -> Self {
        Self {
            root: RegionTree::with_count_ref(),
            subtrees: Default::default(),
            regions: HashMap::new(),
        }
    }
}
impl RegionsInfo {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }
    /// Returns the current payload for an ID.
    pub fn get_region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.regions.get(&id).cloned()
    }
    /// Returns an owned snapshot of all current payloads, in unspecified order.
    pub fn get_regions(&self) -> Vec<Arc<RegionInfo>> {
        self.regions.values().cloned().collect()
    }
    /// Returns the root index entry count.
    pub fn tree_len(&self) -> usize {
        self.root.tree.len()
    }
    /// Replaces an ID and evicts overlapping ranges. Returns the previous ID
    /// payload, evicted payloads in key order, and whether the range changed.
    /// A moved ID is excluded from its own overlap result.
    pub fn set_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> (Option<Arc<RegionInfo>>, Vec<Arc<RegionInfo>>, bool) {
        let id = region.meta().id;
        let origin = self.regions.get(&id).cloned();
        if let Some(old) = &origin {
            if old.meta().key_range == region.meta().key_range {
                self.root.update_stat(old, &region);
                self.root
                    .tree
                    .replace_or_insert(RegionItem::new(region.clone()));
                self.regions.insert(id, region);
                return (origin, vec![], false);
            }
            self.root.remove(old);
        }
        let overlaps = self.root.update(region.clone());
        for old in &overlaps {
            self.regions.remove(&old.meta().id);
        }
        self.regions.insert(id, region);
        (origin, overlaps, true)
    }
}
