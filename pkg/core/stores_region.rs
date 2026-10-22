// Copyright 2026 AsterSQL.
//! Region membership queries against a single locked store snapshot.
use crate::{RegionInfo, StoreInfo, StoresInfo};
use std::sync::Arc;

impl StoresInfo {
    /// Registered stores containing a follower, including witness followers.
    /// Ordering is unspecified, matching Go map iteration.
    pub fn get_follower_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        let stores = self.stores.read().unwrap();
        region
            .followers()
            .keys()
            .filter_map(|id| stores.get(id).cloned())
            .collect()
    }

    /// Registered stores containing any peer, deduplicated by store ID.
    /// Ordering is unspecified.
    pub fn get_region_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        let stores = self.stores.read().unwrap();
        region
            .store_ids()
            .iter()
            .filter_map(|id| stores.get(id).cloned())
            .collect()
    }

    /// Registered leader store. An absent leader uses ID zero, preserving the
    /// Go protobuf getter's default even if a store with ID zero is registered.
    pub fn get_leader_store(&self, region: &RegionInfo) -> Option<Arc<StoreInfo>> {
        let stores = self.stores.read().unwrap();
        let id = region.leader().map_or(0, |peer| peer.store_id);
        stores.get(&id).cloned()
    }

    /// Registered stores containing non-witness voters, including the leader
    /// and peers in joint membership states. Ordering is unspecified.
    pub fn get_non_witness_voter_stores(&self, region: &RegionInfo) -> Vec<Arc<StoreInfo>> {
        let stores = self.stores.read().unwrap();
        region
            .non_witness_voters()
            .keys()
            .filter_map(|id| stores.get(id).cloned())
            .collect()
    }
}
