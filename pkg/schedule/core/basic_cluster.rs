// Copyright 2026 AsterSQL.
//! Scheduling access to the host's canonical store registry and region indexes.
use astersql_pd_core::{
    BasicCluster as CoreCluster, BatchScanRegionsOptions, KeyRange, RegionInfo, RegionNotAdjacent,
    RegionRole, RegionsInfo, StoreControlError, StoreInfo, StoresInfo,
};
use astersql_pd_core_constant::Direction;
use std::sync::Arc;

/// Base observations and store controls shared by schedulers and checkers.
///
/// Both accessors must borrow the same host-owned cluster state. Reads retain
/// canonical Arc payloads; controls use the store registry's copy-on-write lock.
/// Region updates remain with the host, whose exclusive borrow prevents a
/// scheduler observation from racing a partial root/subtree update. The trait
/// owns no cache, runtime, protocol types, or persistence backend.
///
/// This corresponds to Go's StoreSetInformer + StoreSetController +
/// RegionSetInformer. Randomness is supplied by the caller at its input boundary.
pub trait BasicCluster {
    /// The host's canonical store registry, also used for control mutations.
    fn stores(&self) -> &StoresInfo;
    /// The host's canonical root and store-role region indexes.
    fn regions(&self) -> &RegionsInfo;

    /// Current registered stores, with unspecified ordering.
    fn get_stores(&self) -> Vec<Arc<StoreInfo>> {
        self.stores().get_stores()
    }
    /// A registered immutable store snapshot, or None when missing.
    fn get_store(&self, id: u64) -> Option<Arc<StoreInfo>> {
        self.stores().get_store(id)
    }
    /// Registered stores containing any region peer.
    fn get_region_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        self.stores().get_region_stores(region)
    }
    /// Registered stores containing non-witness voters.
    fn get_non_witness_voter_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        self.stores().get_non_witness_voter_stores(region)
    }
    /// Registered stores containing followers, including witnesses.
    fn get_follower_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        self.stores().get_follower_stores(region)
    }
    /// Registered leader store; absent leaders follow the core ID-zero rule.
    fn get_leader_store(&self, region: &RegionInfo) -> Option<Arc<StoreInfo>> {
        self.stores().get_leader_store(region)
    }
    /// Average score over other registered stores, including default scores.
    fn avg_network_slow_score(&self, id: u64) -> u64 {
        self.stores().avg_network_slow_score(id)
    }
    /// Add one directional leader-transfer pause, rejecting absent stores.
    fn pause_leader_transfer(
        &self,
        id: u64,
        direction: Direction,
    ) -> Result<(), StoreControlError> {
        self.stores().pause_leader_transfer(id, direction)
    }
    /// Release one directional pause; absent stores are ignored.
    fn resume_leader_transfer(&self, id: u64, direction: Direction) {
        self.stores().resume_leader_transfer(id, direction);
    }
    /// Evict a slow-store; preserves missing-store and duplicate error semantics.
    fn slow_store_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.stores().slow_store_evicted(id)
    }
    /// Recover a slow-store; absent stores are ignored.
    fn slow_store_recovered(&self, id: u64) {
        self.stores().slow_store_recovered(id);
    }
    /// Evict a stopping-store; preserves missing-store and duplicate error semantics.
    fn stopping_store_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.stores().stopping_store_evicted(id)
    }
    /// Recover a stopping-store; absent stores are ignored.
    fn stopping_store_recovered(&self, id: u64) {
        self.stores().stopping_store_recovered(id);
    }
    /// Evict a slow-trend; preserves missing-store and duplicate error semantics.
    fn slow_trend_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.stores().slow_trend_evicted(id)
    }
    /// Recover a slow-trend; absent stores are ignored.
    fn slow_trend_recovered(&self, id: u64) {
        self.stores().slow_trend_recovered(id);
    }
    /// Number of regions in the canonical root index.
    fn total_region_count(&self) -> usize {
        self.regions().total_region_count()
    }
    /// Current region payload, or None for a missing ID.
    fn get_region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.regions().get_region(id)
    }
    /// Integer mean region size in MiB; zero for an empty cluster.
    fn average_region_size(&self) -> i64 {
        self.regions().average_region_size()
    }
    /// Leader + follower + learner membership count for a store.
    fn store_region_count(&self, id: u64) -> usize {
        self.regions().store_region_count(id)
    }
    /// Neighboring regions only when their range endpoints touch.
    fn get_adjacent_regions(
        &self,
        region: &RegionInfo,
    ) -> (Option<Arc<RegionInfo>>, Option<Arc<RegionInfo>>) {
        self.regions().get_adjacent_regions(region)
    }
    /// Ordered range scan with Go-compatible limits and half-open bounds.
    fn scan_regions(&self, start: &[u8], end: &[u8], limit: isize) -> Vec<Arc<RegionInfo>> {
        self.regions().scan_regions(start, end, limit)
    }
    /// Region containing the key; a hole must not return its next region.
    fn get_region_by_key(&self, key: &[u8]) -> Option<Arc<RegionInfo>> {
        self.regions()
            .scan_regions(key, b"", 1)
            .into_iter()
            .find(|region| region.meta().key_range.contains(key))
    }
    /// Batch scan retaining core's typed gap errors and coverage rules.
    fn batch_scan_regions(
        &self,
        ranges: &[KeyRange],
        options: BatchScanRegionsOptions,
    ) -> Result<Vec<Arc<RegionInfo>>, RegionNotAdjacent> {
        self.regions().batch_scan_regions(ranges, options)
    }
    /// Signed rank count, preserving the core's reversed-bound semantics.
    fn region_count_by_range(&self, start: &[u8], end: &[u8]) -> isize {
        self.regions().get_region_count_by_range(start, end)
    }
    /// Count a store's leaders within the requested range.
    fn store_leader_count_by_range(&self, id: u64, start: &[u8], end: &[u8]) -> isize {
        self.regions()
            .store_role_count_by_range(id, RegionRole::Leader, start, end)
    }
    /// Count a store's learners within the requested range.
    fn store_learner_count_by_range(&self, id: u64, start: &[u8], end: &[u8]) -> isize {
        self.regions()
            .store_role_count_by_range(id, RegionRole::Learner, start, end)
    }
    /// Count peers without double counting witnesses or pending membership.
    fn store_peer_count_by_range(&self, id: u64, start: &[u8], end: &[u8]) -> isize {
        self.regions().store_peer_count_by_range(id, start, end)
    }
    /// Sample follower regions using caller-owned random inputs and core bounds.
    fn rand_follower_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.regions()
            .rand_follower_regions(id, ranges, index, shuffle)
    }
    /// Sample leader regions using caller-owned random inputs and core bounds.
    fn rand_leader_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.regions()
            .rand_leader_regions(id, ranges, index, shuffle)
    }
    /// Sample learner regions using caller-owned random inputs and core bounds.
    fn rand_learner_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.regions()
            .rand_learner_regions(id, ranges, index, shuffle)
    }
    /// Sample witness regions using caller-owned random inputs and core bounds.
    fn rand_witness_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.regions()
            .rand_witness_regions(id, ranges, index, shuffle)
    }
    /// Sample pending regions using caller-owned random inputs and core bounds.
    fn rand_pending_regions(
        &self,
        id: u64,
        ranges: &[KeyRange],
        index: &mut dyn FnMut(usize) -> usize,
        shuffle: &mut dyn FnMut(&mut [usize]),
    ) -> Vec<Arc<RegionInfo>> {
        self.regions()
            .rand_pending_regions(id, ranges, index, shuffle)
    }
}

impl BasicCluster for CoreCluster {
    fn stores(&self) -> &StoresInfo {
        &self.stores
    }
    fn regions(&self) -> &RegionsInfo {
        &self.regions
    }
}
