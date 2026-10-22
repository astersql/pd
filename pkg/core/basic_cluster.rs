// Copyright 2026 AsterSQL.
//! Composition of the canonical store registry and region indexes.
use crate::{RegionsInfo, StoreInfo, StoresInfo};
use std::sync::Arc;

/// Basic cluster state; region mutations require exclusive access, while store
/// snapshots and status updates use the registry's internal lock.
#[derive(Default)]
pub struct BasicCluster {
    pub stores: StoresInfo,
    pub regions: RegionsInfo,
}
impl BasicCluster {
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy the seven indexed counters into a shallow store snapshot. Unknown
    /// stores are ignored, including entries deleted before acquiring the lock.
    pub fn update_store_status(&self, store_id: u64) {
        let stats = self.regions.store_stats(store_id);
        let mut stores = self.stores.stores.write().unwrap();
        if let Some(current) = stores.get(&store_id) {
            let mut next = current.shallow_clone();
            next.set_leader_count(stats.leader as i64);
            next.set_region_count(stats.region as i64);
            next.set_witness_count(stats.witness as i64);
            next.set_learner_count(stats.learner as i64);
            next.set_pending_peer_count(stats.pending as i64);
            next.set_leader_size(stats.leader_size);
            next.set_region_size(stats.region_size);
            stores.insert(store_id, Arc::new(next));
        }
    }

    /// Refresh registered stores except those whose node state is Removed.
    pub fn update_all_store_status(&self) {
        for store in self.stores.get_stores() {
            if !store.is_removed() {
                self.update_store_status(store.id());
            }
        }
    }

    /// Return the registered leader store, requiring an actual region leader.
    pub fn get_leader_store_by_region_id(&self, region_id: u64) -> Option<Arc<StoreInfo>> {
        let region = self.regions.get_region(region_id)?;
        self.stores.get_store(region.leader()?.store_id)
    }
}
