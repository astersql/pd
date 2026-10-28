// Copyright 2026 AsterSQL.
//! Store-role sampling with caller-owned randomness.
use crate::{KeyRange, RegionInfo, RegionRole, RegionState};
use std::sync::Arc;

/// Whether the separately recorded leader is a witness and needs transfer.
/// Missing regions and leaders do not require transfer.
pub fn need_transfer_witness_leader(region: Option<&RegionInfo>) -> bool {
    region
        .and_then(RegionInfo::leader)
        .is_some_and(|peer| peer.is_witness)
}

impl RegionState {
    /// Samples up to ten fully contained leader regions with replacement.
    /// Empty ranges cover the whole role index. Random callbacks follow
    /// [`crate::RegionTree::random_regions`], including its retry semantics.
    pub fn rand_leader_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.role_tree(RegionRole::Leader, store)
            .map_or_else(Vec::new, |tree| {
                tree.random_regions(10, ranges, index, shuffle)
            })
    }
    /// Samples up to ten fully contained follower regions with replacement.
    /// Empty ranges cover the whole role index. Random callbacks follow
    /// [`crate::RegionTree::random_regions`], including its retry semantics.
    pub fn rand_follower_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.role_tree(RegionRole::Follower, store)
            .map_or_else(Vec::new, |tree| {
                tree.random_regions(10, ranges, index, shuffle)
            })
    }
    /// Samples up to ten fully contained learner regions with replacement.
    /// Empty ranges cover the whole role index. Random callbacks follow
    /// [`crate::RegionTree::random_regions`], including its retry semantics.
    pub fn rand_learner_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.role_tree(RegionRole::Learner, store)
            .map_or_else(Vec::new, |tree| {
                tree.random_regions(10, ranges, index, shuffle)
            })
    }
    /// Samples up to ten fully contained witness regions with replacement.
    /// Empty ranges cover the whole role index. Random callbacks follow
    /// [`crate::RegionTree::random_regions`], including its retry semantics.
    pub fn rand_witness_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.role_tree(RegionRole::Witness, store)
            .map_or_else(Vec::new, |tree| {
                tree.random_regions(10, ranges, index, shuffle)
            })
    }
    /// Samples up to ten fully contained pending regions with replacement.
    /// Empty ranges cover the whole role index. Random callbacks follow
    /// [`crate::RegionTree::random_regions`], including its retry semantics.
    pub fn rand_pending_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.role_tree(RegionRole::Pending, store)
            .map_or_else(Vec::new, |tree| {
                tree.random_regions(10, ranges, index, shuffle)
            })
    }
}
