// Copyright 2026 AsterSQL.
//! Registry controls serialize membership lookup and copy-on-write updates.
use crate::{StoreInfo, StoresInfo};
use astersql_pd_core_constant::Direction;
use astersql_pd_storelimit::Type;
use std::sync::Arc;

/// Stable domain errors corresponding to Go ErrStoreNotFound/ErrSlowTrendEvicted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreControlError {
    StoreNotFound(u64),
    SlowTrendEvicted(u64),
}
impl std::fmt::Display for StoreControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StoreNotFound(id) => write!(f, "store {id} not found"),
            Self::SlowTrendEvicted(id) => write!(f, "store {id} already evicted as slow trend"),
        }
    }
}
impl std::error::Error for StoreControlError {}
impl StoreInfo {
    pub fn network_slow_triggers(&self) -> u64 {
        self.network_slow_triggers
    }
}
impl StoresInfo {
    fn control_store(
        &self,
        id: u64,
        patch: impl FnOnce(&mut StoreInfo) -> Result<(), StoreControlError>,
    ) -> Result<(), StoreControlError> {
        let mut stores = self.stores.write().unwrap();
        let current = stores
            .get(&id)
            .ok_or(StoreControlError::StoreNotFound(id))?;
        let mut next = current.as_ref().clone();
        patch(&mut next)?;
        stores.insert(id, Arc::new(next));
        Ok(())
    }
    pub fn pause_leader_transfer(
        &self,
        id: u64,
        direction: Direction,
    ) -> Result<(), StoreControlError> {
        self.control_store(id, |s| {
            s.pause_leader_transfer(direction);
            Ok(())
        })
    }
    pub fn resume_leader_transfer(&self, id: u64, direction: Direction) {
        self.patch_store(id, |s| s.resume_leader_transfer(direction));
    }
    pub fn slow_store_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.control_store(id, |s| {
            s.slow_store_evicted();
            Ok(())
        })
    }
    pub fn slow_store_recovered(&self, id: u64) {
        self.patch_store(id, |s| s.slow_store_recovered());
    }
    pub fn stopping_store_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.control_store(id, |s| {
            s.stopping_store_evicted();
            Ok(())
        })
    }
    pub fn stopping_store_recovered(&self, id: u64) {
        self.patch_store(id, |s| s.stopping_store_recovered());
    }
    pub fn slow_trend_evicted(&self, id: u64) -> Result<(), StoreControlError> {
        self.control_store(id, |s| {
            if s.is_evicted_as_slow_trend() {
                return Err(StoreControlError::SlowTrendEvicted(id));
            }
            s.slow_trend_evicted();
            Ok(())
        })
    }
    pub fn slow_trend_recovered(&self, id: u64) {
        self.patch_store(id, |s| s.slow_trend_recovered());
    }
    pub fn reset_store_limit(&self, id: u64, typ: Type, rates: &[f64]) {
        self.patch_store(id, |s| s.reset_store_limit(typ, rates));
    }
    pub fn trigger_network_slow_evict(&self, id: u64) {
        self.patch_network_triggers(id, |s| {
            s.network_slow_triggers = s.network_slow_triggers.wrapping_add(1)
        });
    }
    pub fn reset_trigger_network_slow_evict(&self, id: u64) {
        self.patch_network_triggers(id, |s| s.network_slow_triggers = 0);
    }
    fn patch_network_triggers(&self, id: u64, patch: impl FnOnce(&mut StoreInfo)) {
        let mut stores = self.stores.write().unwrap();
        if let Some(current) = stores.get(&id) {
            let mut next = current.shallow_clone();
            patch(&mut next);
            stores.insert(id, Arc::new(next));
        }
    }
    pub fn avg_network_slow_score(&self, id: u64) -> u64 {
        let stores = self.stores.read().unwrap();
        let Some(store) = stores.get(&id) else {
            return 0;
        };
        let scores = store.network_slow_scores();
        let mut total = 0u64;
        let mut count = 0;
        for peer in stores.keys().filter(|peer| **peer != id) {
            total = total.wrapping_add(scores.get(peer).copied().unwrap_or(1));
            count += 1;
        }
        if count == 0 { 0 } else { total / count }
    }
}
