// Copyright 2026 AsterSQL.
//! Aligned store IDs and aggregate write rates from the cluster indexes.
use crate::{BasicCluster, RegionRole};

impl BasicCluster {
    fn stores_write_rates(&self, leader_only: bool) -> (Vec<u64>, Vec<f64>, Vec<f64>) {
        let stores = self.stores.get_stores();
        let mut ids = Vec::with_capacity(stores.len());
        let mut bytes = Vec::with_capacity(stores.len());
        let mut keys = Vec::with_capacity(stores.len());
        for store in stores {
            if store.is_removed() {
                continue;
            }
            let id = store.id();
            let (b, k) = if leader_only {
                self.regions.store_role_write_rate(id, RegionRole::Leader)
            } else {
                self.regions.store_write_rate(id)
            };
            ids.push(id);
            bytes.push(b);
            keys.push(k);
        }
        (ids, bytes, keys)
    }

    /// Return aligned store IDs, leader write bytes/s and leader write keys/s.
    /// Removed stores are omitted; stores without indexed leaders have zero rates.
    /// Store order is unspecified, but each index refers to the same store.
    pub fn get_stores_leader_write_rate(&self) -> (Vec<u64>, Vec<f64>, Vec<f64>) {
        self.stores_write_rates(true)
    }

    /// Return aligned IDs, write bytes/s and keys/s for registered stores.
    /// Sum leader, follower and learner rates, omitting Removed stores.
    /// Missing region indexes contribute zero; store order is unspecified.
    pub fn get_stores_write_rate(&self) -> (Vec<u64>, Vec<f64>, Vec<f64>) {
        self.stores_write_rates(false)
    }
}
