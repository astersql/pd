// Copyright 2026 AsterSQL.
//! Shared canonical indexes. Each operation locks all root and role indexes together.
use crate::*;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Instant;

/// Clones are handles to the same index generation, never copied region caches.
#[derive(Clone, Default)]
pub struct RegionsInfo {
    inner: Arc<RwLock<RegionState>>,
}
/// A role tree read retains the index lock until the view is dropped.
pub struct RegionRoleTreeGuard<'a> {
    state: RwLockReadGuard<'a, RegionState>,
    role: RegionRole,
    store: u64,
}
impl std::ops::Deref for RegionRoleTreeGuard<'_> {
    type Target = RegionTree;
    fn deref(&self) -> &RegionTree {
        self.state.role_tree(self.role, self.store).unwrap()
    }
}
impl RegionsInfo {
    /// Bind one host-owned collector set before serving requests.
    pub fn set_metrics(&self, metrics: Arc<crate::metrics::CoreMetrics>) {
        self.inner.write().unwrap().metrics = metrics;
    }
    pub fn metrics(&self) -> Arc<crate::metrics::CoreMetrics> {
        self.inner.read().unwrap().metrics.clone()
    }
    fn read_metrics(&self, subtree: bool) -> RwLockReadGuard<'_, RegionState> {
        let begin = Instant::now();
        let state = self.inner.read().unwrap();
        state.metrics.observe_wait_metrics(subtree, begin.elapsed());
        state
    }
    fn write_metrics(&self, subtree: bool) -> RwLockWriteGuard<'_, RegionState> {
        let begin = Instant::now();
        let state = self.inner.write().unwrap();
        state.metrics.observe_wait_metrics(subtree, begin.elapsed());
        state
    }
    pub fn new() -> Self {
        Self::default()
    }
    #[cfg(test)]
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, RegionState> {
        self.inner.read().unwrap()
    }
    pub fn role_tree(&self, role: RegionRole, store: u64) -> Option<RegionRoleTreeGuard<'_>> {
        let state = self.read_metrics(true);
        state.role_tree(role, store)?;
        Some(RegionRoleTreeGuard { state, role, store })
    }
    pub fn get_region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.read_metrics(false).get_region(id)
    }
    /// Return an immutable point-query snapshot under one canonical index lock.
    pub fn region_by_key(&self, key: &[u8], previous: bool) -> Option<Arc<RegionInfo>> {
        let state = self.read_metrics(false);
        if previous {
            state.root.search_prev(key)
        } else {
            state.root.search(key)
        }
    }
    pub fn get_regions(&self) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(false).get_regions()
    }
    /// Snapshot store membership from the canonical role indexes under one lock.
    /// All membership is the union of leaders, followers and learners, as in Go.
    pub fn store_regions(&self, store: u64, role: Option<RegionRole>) -> Vec<Arc<RegionInfo>> {
        let state = self.read_metrics(true);
        let roles = role.map_or_else(
            || {
                vec![
                    RegionRole::Leader,
                    RegionRole::Follower,
                    RegionRole::Learner,
                ]
            },
            |role| vec![role],
        );
        roles
            .into_iter()
            .flat_map(|role| {
                state
                    .role_tree(role, store)
                    .map_or_else(Vec::new, |tree| tree.scan_ranges())
            })
            .collect()
    }
    pub fn tree_len(&self) -> usize {
        self.read_metrics(false).tree_len()
    }
    pub fn set_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> (Option<Arc<RegionInfo>>, Vec<Arc<RegionInfo>>, bool) {
        self.write_metrics(false).set_region(region)
    }
    pub fn store_role_count(&self, store: u64, role: RegionRole) -> usize {
        self.read_metrics(true).store_role_count(store, role)
    }
    pub fn store_role_size(&self, store: u64, role: RegionRole) -> i64 {
        self.read_metrics(true).store_role_size(store, role)
    }
    pub fn store_role_write_rate(&self, store: u64, role: RegionRole) -> (f64, f64) {
        self.read_metrics(true).store_role_write_rate(store, role)
    }
    pub fn store_region_count(&self, store: u64) -> usize {
        self.read_metrics(true).store_region_count(store)
    }
    pub fn store_region_size(&self, store: u64) -> i64 {
        self.read_metrics(true).store_region_size(store)
    }
    pub fn store_write_rate(&self, store: u64) -> (f64, f64) {
        self.read_metrics(true).store_write_rate(store)
    }
    pub fn store_stats(&self, store: u64) -> StoreRegionStats {
        self.read_metrics(true).store_stats(store)
    }
    pub fn total_region_count(&self) -> usize {
        self.read_metrics(false).total_region_count()
    }
    pub fn average_region_size(&self) -> i64 {
        self.read_metrics(false).average_region_size()
    }
    pub fn not_from_storage_regions_count(&self) -> i64 {
        self.read_metrics(false).not_from_storage_regions_count()
    }
    pub fn not_from_storage_regions_count_by_store(&self, store: u64) -> i64 {
        self.read_metrics(false)
            .not_from_storage_regions_count_by_store(store)
    }
    pub fn is_store_prepared(&self, store: u64) -> bool {
        self.read_metrics(false).is_store_prepared(store)
    }
    pub fn check_and_put_sub_tree(&mut self, event: &RegionInfo) {
        self.write_metrics(true).check_and_put_sub_tree(event)
    }
    pub fn update_sub_tree_order_insensitive(&mut self, region: Arc<RegionInfo>) {
        self.write_metrics(true)
            .update_sub_tree_order_insensitive(region)
    }
    pub fn rand_leader_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(true)
            .rand_leader_regions(store, ranges, index, shuffle)
    }
    pub fn rand_follower_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(true)
            .rand_follower_regions(store, ranges, index, shuffle)
    }
    pub fn rand_learner_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(true)
            .rand_learner_regions(store, ranges, index, shuffle)
    }
    pub fn rand_witness_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(true)
            .rand_witness_regions(store, ranges, index, shuffle)
    }
    pub fn rand_pending_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(true)
            .rand_pending_regions(store, ranges, index, shuffle)
    }
    pub fn pre_check_put_region(&self, region: &RegionInfo) -> RegionPreCheck {
        self.read_metrics(false).pre_check_put_region(region)
    }
    pub fn valid_region(&self, region: &RegionMeta) -> Result<(), RegionValidationError> {
        self.read_metrics(false).valid_region(region)
    }
    pub fn batch_scan_regions(
        &self,
        ranges: &[KeyRange],
        options: BatchScanRegionsOptions,
    ) -> Result<Vec<Arc<RegionInfo>>, RegionNotAdjacent> {
        self.read_metrics(false).batch_scan_regions(ranges, options)
    }
    pub fn remove_region(&mut self, region: &RegionInfo) {
        self.write_metrics(false).remove_region(region)
    }
    pub fn remove_region_from_sub_tree(&mut self, region: &RegionInfo) {
        self.write_metrics(true).remove_region_from_sub_tree(region)
    }
    pub fn remove_region_if_exist(&mut self, id: u64) {
        self.write_metrics(false).remove_region_if_exist(id)
    }
    pub fn reset_region_cache(&mut self) {
        self.write_metrics(false).reset_region_cache()
    }
    pub fn update_sub_tree(
        &mut self,
        region: Arc<RegionInfo>,
        origin: Option<&RegionInfo>,
        overlaps: &[Arc<RegionInfo>],
        range_changed: bool,
    ) {
        self.write_metrics(true)
            .update_sub_tree(region, origin, overlaps, range_changed)
    }
    pub fn query_regions(
        &self,
        keys: &[&[u8]],
        prev_keys: &[&[u8]],
        ids: &[u64],
        need_buckets: bool,
    ) -> RegionQueryResult {
        self.read_metrics(false)
            .query_regions(keys, prev_keys, ids, need_buckets)
    }
    pub fn get_region_size_by_range(&self, start: &[u8], end: &[u8]) -> i64 {
        self.read_metrics(false)
            .get_region_size_by_range(start, end)
    }
    pub fn get_region_count_by_range(&self, start: &[u8], end: &[u8]) -> isize {
        self.read_metrics(false)
            .get_region_count_by_range(start, end)
    }
    pub fn store_role_count_by_range(
        &self,
        store: u64,
        role: RegionRole,
        start: &[u8],
        end: &[u8],
    ) -> isize {
        self.read_metrics(true)
            .store_role_count_by_range(store, role, start, end)
    }
    pub fn store_peer_count_by_range(&self, store: u64, start: &[u8], end: &[u8]) -> isize {
        self.read_metrics(true)
            .store_peer_count_by_range(store, start, end)
    }
    pub fn get_adjacent_regions(
        &self,
        region: &RegionInfo,
    ) -> (Option<Arc<RegionInfo>>, Option<Arc<RegionInfo>>) {
        self.read_metrics(false).get_adjacent_regions(region)
    }
    pub fn get_range_holes(&self) -> Vec<[String; 2]> {
        self.read_metrics(false).get_range_holes()
    }
    pub fn put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.write_metrics(false).put_region(region)
    }
    pub fn check_and_put_root_tree(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.write_metrics(false).check_and_put_root_tree(region)
    }
    pub fn atomic_check_and_put_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.write_metrics(false)
            .atomic_check_and_put_region(region)
    }
    pub fn check_and_put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.write_metrics(false).check_and_put_region(region)
    }
    pub fn scan_regions(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        limit: isize,
    ) -> Vec<Arc<RegionInfo>> {
        self.read_metrics(false)
            .scan_regions(start_key, end_key, limit)
    }
    pub fn scan_region_with_iterator(
        &self,
        start_key: &[u8],
        iterator: impl FnMut(&Arc<RegionInfo>) -> bool,
    ) {
        self.read_metrics(false)
            .scan_region_with_iterator(start_key, iterator)
    }
}
