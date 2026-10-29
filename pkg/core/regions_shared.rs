// Copyright 2026 AsterSQL.
//! Shared canonical indexes. Each operation locks all root and role indexes together.
use crate::*;
use std::sync::{Arc, RwLock, RwLockReadGuard};

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
    pub fn new() -> Self {
        Self::default()
    }
    #[cfg(test)]
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, RegionState> {
        self.inner.read().unwrap()
    }
    pub fn role_tree(&self, role: RegionRole, store: u64) -> Option<RegionRoleTreeGuard<'_>> {
        let state = self.inner.read().unwrap();
        state.role_tree(role, store)?;
        Some(RegionRoleTreeGuard { state, role, store })
    }
    pub fn get_region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.inner.read().unwrap().get_region(id)
    }
    /// Return an immutable point-query snapshot under one canonical index lock.
    pub fn region_by_key(&self, key: &[u8], previous: bool) -> Option<Arc<RegionInfo>> {
        let state = self.inner.read().unwrap();
        if previous {
            state.root.search_prev(key)
        } else {
            state.root.search(key)
        }
    }
    pub fn get_regions(&self) -> Vec<Arc<RegionInfo>> {
        self.inner.read().unwrap().get_regions()
    }
    pub fn tree_len(&self) -> usize {
        self.inner.read().unwrap().tree_len()
    }
    pub fn set_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> (Option<Arc<RegionInfo>>, Vec<Arc<RegionInfo>>, bool) {
        self.inner.write().unwrap().set_region(region)
    }
    pub fn store_role_count(&self, store: u64, role: RegionRole) -> usize {
        self.inner.read().unwrap().store_role_count(store, role)
    }
    pub fn store_role_size(&self, store: u64, role: RegionRole) -> i64 {
        self.inner.read().unwrap().store_role_size(store, role)
    }
    pub fn store_role_write_rate(&self, store: u64, role: RegionRole) -> (f64, f64) {
        self.inner
            .read()
            .unwrap()
            .store_role_write_rate(store, role)
    }
    pub fn store_region_count(&self, store: u64) -> usize {
        self.inner.read().unwrap().store_region_count(store)
    }
    pub fn store_region_size(&self, store: u64) -> i64 {
        self.inner.read().unwrap().store_region_size(store)
    }
    pub fn store_write_rate(&self, store: u64) -> (f64, f64) {
        self.inner.read().unwrap().store_write_rate(store)
    }
    pub fn store_stats(&self, store: u64) -> StoreRegionStats {
        self.inner.read().unwrap().store_stats(store)
    }
    pub fn total_region_count(&self) -> usize {
        self.inner.read().unwrap().total_region_count()
    }
    pub fn average_region_size(&self) -> i64 {
        self.inner.read().unwrap().average_region_size()
    }
    pub fn not_from_storage_regions_count(&self) -> i64 {
        self.inner.read().unwrap().not_from_storage_regions_count()
    }
    pub fn not_from_storage_regions_count_by_store(&self, store: u64) -> i64 {
        self.inner
            .read()
            .unwrap()
            .not_from_storage_regions_count_by_store(store)
    }
    pub fn is_store_prepared(&self, store: u64) -> bool {
        self.inner.read().unwrap().is_store_prepared(store)
    }
    pub fn check_and_put_sub_tree(&mut self, event: &RegionInfo) {
        self.inner.write().unwrap().check_and_put_sub_tree(event)
    }
    pub fn update_sub_tree_order_insensitive(&mut self, region: Arc<RegionInfo>) {
        self.inner
            .write()
            .unwrap()
            .update_sub_tree_order_insensitive(region)
    }
    pub fn rand_leader_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .rand_leader_regions(store, ranges, index, shuffle)
    }
    pub fn rand_follower_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .rand_follower_regions(store, ranges, index, shuffle)
    }
    pub fn rand_learner_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .rand_learner_regions(store, ranges, index, shuffle)
    }
    pub fn rand_witness_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .rand_witness_regions(store, ranges, index, shuffle)
    }
    pub fn rand_pending_regions(
        &self,
        store: u64,
        ranges: &[KeyRange],
        index: impl FnMut(usize) -> usize,
        shuffle: impl FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .rand_pending_regions(store, ranges, index, shuffle)
    }
    pub fn pre_check_put_region(&self, region: &RegionInfo) -> RegionPreCheck {
        self.inner.read().unwrap().pre_check_put_region(region)
    }
    pub fn valid_region(&self, region: &RegionMeta) -> Result<(), RegionValidationError> {
        self.inner.read().unwrap().valid_region(region)
    }
    pub fn batch_scan_regions(
        &self,
        ranges: &[KeyRange],
        options: BatchScanRegionsOptions,
    ) -> Result<Vec<Arc<RegionInfo>>, RegionNotAdjacent> {
        self.inner
            .read()
            .unwrap()
            .batch_scan_regions(ranges, options)
    }
    pub fn remove_region(&mut self, region: &RegionInfo) {
        self.inner.write().unwrap().remove_region(region)
    }
    pub fn remove_region_from_sub_tree(&mut self, region: &RegionInfo) {
        self.inner
            .write()
            .unwrap()
            .remove_region_from_sub_tree(region)
    }
    pub fn remove_region_if_exist(&mut self, id: u64) {
        self.inner.write().unwrap().remove_region_if_exist(id)
    }
    pub fn reset_region_cache(&mut self) {
        self.inner.write().unwrap().reset_region_cache()
    }
    pub fn update_sub_tree(
        &mut self,
        region: Arc<RegionInfo>,
        origin: Option<&RegionInfo>,
        overlaps: &[Arc<RegionInfo>],
        range_changed: bool,
    ) {
        self.inner
            .write()
            .unwrap()
            .update_sub_tree(region, origin, overlaps, range_changed)
    }
    pub fn query_regions(
        &self,
        keys: &[&[u8]],
        prev_keys: &[&[u8]],
        ids: &[u64],
        need_buckets: bool,
    ) -> RegionQueryResult {
        self.inner
            .read()
            .unwrap()
            .query_regions(keys, prev_keys, ids, need_buckets)
    }
    pub fn get_region_size_by_range(&self, start: &[u8], end: &[u8]) -> i64 {
        self.inner
            .read()
            .unwrap()
            .get_region_size_by_range(start, end)
    }
    pub fn get_region_count_by_range(&self, start: &[u8], end: &[u8]) -> isize {
        self.inner
            .read()
            .unwrap()
            .get_region_count_by_range(start, end)
    }
    pub fn store_role_count_by_range(
        &self,
        store: u64,
        role: RegionRole,
        start: &[u8],
        end: &[u8],
    ) -> isize {
        self.inner
            .read()
            .unwrap()
            .store_role_count_by_range(store, role, start, end)
    }
    pub fn store_peer_count_by_range(&self, store: u64, start: &[u8], end: &[u8]) -> isize {
        self.inner
            .read()
            .unwrap()
            .store_peer_count_by_range(store, start, end)
    }
    pub fn get_adjacent_regions(
        &self,
        region: &RegionInfo,
    ) -> (Option<Arc<RegionInfo>>, Option<Arc<RegionInfo>>) {
        self.inner.read().unwrap().get_adjacent_regions(region)
    }
    pub fn get_range_holes(&self) -> Vec<[String; 2]> {
        self.inner.read().unwrap().get_range_holes()
    }
    pub fn put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.inner.write().unwrap().put_region(region)
    }
    pub fn check_and_put_root_tree(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.inner.write().unwrap().check_and_put_root_tree(region)
    }
    pub fn atomic_check_and_put_region(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<Vec<Arc<RegionInfo>>, StaleRegionError> {
        self.inner
            .write()
            .unwrap()
            .atomic_check_and_put_region(region)
    }
    pub fn check_and_put_region(&mut self, region: Arc<RegionInfo>) -> Vec<Arc<RegionInfo>> {
        self.inner.write().unwrap().check_and_put_region(region)
    }
    pub fn scan_regions(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        limit: isize,
    ) -> Vec<Arc<RegionInfo>> {
        self.inner
            .read()
            .unwrap()
            .scan_regions(start_key, end_key, limit)
    }
    pub fn scan_region_with_iterator(
        &self,
        start_key: &[u8],
        iterator: impl FnMut(&Arc<RegionInfo>) -> bool,
    ) {
        self.inner
            .read()
            .unwrap()
            .scan_region_with_iterator(start_key, iterator)
    }
}
