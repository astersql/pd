// Copyright 2026 AsterSQL.
//! Aggregate views of the canonical root and store-role indexes.
use crate::{RegionRole, RegionState};

const PEER_ROLES: [RegionRole; 3] = [
    RegionRole::Leader,
    RegionRole::Follower,
    RegionRole::Learner,
];

/// Counts and sizes observed together from a store's role indexes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoreRegionStats {
    pub leader: usize,
    pub region: usize,
    pub witness: usize,
    pub learner: usize,
    pub pending: usize,
    pub leader_size: i64,
    pub region_size: i64,
}

impl RegionState {
    /// Returns a role's count, including zero for an unknown store.
    pub fn store_role_count(&self, store: u64, role: RegionRole) -> usize {
        self.role_tree(role, store).map_or(0, |t| t.tree.len())
    }
    /// Returns the approximate size of regions with this role.
    pub fn store_role_size(&self, store: u64, role: RegionRole) -> i64 {
        self.role_tree(role, store).map_or(0, |t| t.total_size())
    }
    /// Returns the role's aggregate write bytes and keys per second.
    pub fn store_role_write_rate(&self, store: u64, role: RegionRole) -> (f64, f64) {
        self.role_tree(role, store)
            .map_or((0., 0.), |t| t.total_write_rate())
    }
    /// Counts leader, follower and learner membership once each.
    pub fn store_region_count(&self, store: u64) -> usize {
        PEER_ROLES
            .iter()
            .map(|role| self.store_role_count(store, *role))
            .sum()
    }
    /// Sums leader, follower and learner sizes with Go signed wrapping.
    pub fn store_region_size(&self, store: u64) -> i64 {
        PEER_ROLES.iter().fold(0i64, |sum, role| {
            sum.wrapping_add(self.store_role_size(store, *role))
        })
    }
    /// Sums leader, follower and learner write rates.
    pub fn store_write_rate(&self, store: u64) -> (f64, f64) {
        PEER_ROLES.iter().fold((0., 0.), |sum, role| {
            let rate = self.store_role_write_rate(store, *role);
            (sum.0 + rate.0, sum.1 + rate.1)
        })
    }
    /// Returns counts and sizes from the same borrowed index snapshot.
    pub fn store_stats(&self, store: u64) -> StoreRegionStats {
        StoreRegionStats {
            leader: self.store_role_count(store, RegionRole::Leader),
            region: self.store_region_count(store),
            witness: self.store_role_count(store, RegionRole::Witness),
            learner: self.store_role_count(store, RegionRole::Learner),
            pending: self.store_role_count(store, RegionRole::Pending),
            leader_size: self.store_role_size(store, RegionRole::Leader),
            region_size: self.store_region_size(store),
        }
    }
    /// Returns the current root registry count.
    pub fn total_region_count(&self) -> usize {
        self.regions.len()
    }
    /// Returns integer mean approximate size, or zero for an empty root.
    pub fn average_region_size(&self) -> i64 {
        if self.tree_len() == 0 {
            0
        } else {
            self.root.total_size() / self.tree_len() as i64
        }
    }
    /// Counts root regions whose current source is not storage.
    pub fn not_from_storage_regions_count(&self) -> i64 {
        self.root.not_from_storage_regions_count()
    }
    /// Counts active leader, follower and learner membership.
    pub fn not_from_storage_regions_count_by_store(&self, store: u64) -> i64 {
        PEER_ROLES
            .iter()
            .map(|role| {
                self.role_tree(*role, store)
                    .map_or(0, |t| t.not_from_storage_regions_count())
            })
            .sum()
    }
    /// A store is prepared when at least 90% of its regions are active.
    /// As in Go, an empty store is prepared.
    pub fn is_store_prepared(&self, store: u64) -> bool {
        self.not_from_storage_regions_count_by_store(store) as f64
            >= self.store_region_count(store) as f64 * 0.9
    }
}
