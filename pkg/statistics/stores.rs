// Copyright 2026 AsterSQL.
//! Store load lifecycle and detached scheduling snapshots.
use crate::RollingStoreStats;
use astersql_pd_core::{StoreStats, StoresInfo};
use astersql_pd_statistics_utils::{STORE_LOAD_COUNT, StoreLoadKind};
use std::{collections::HashMap, sync::RwLock};

/// Owned values in StoreLoadKind order; modifying them never changes the cache.
pub type StoreKindLoads = [f64; STORE_LOAD_COUNT];
/// Read-only access to detached store load values.
pub trait StoreStatInformer {
    fn get_stores_loads(&self) -> HashMap<u64, StoreKindLoads>;
}
/// Synchronized rolling histories, with no externally mutable cache references.
#[derive(Default)]
pub struct StoresStats {
    rolling: RwLock<HashMap<u64, RollingStoreStats>>,
}
impl StoresStats {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn remove_rolling_store_stats(&self, id: u64) {
        self.rolling.write().unwrap().remove(&id);
    }
    /// Returns an independent history copy, including all filter windows.
    pub fn get_rolling_store_stats(&self, id: u64) -> Option<RollingStoreStats> {
        self.rolling.read().unwrap().get(&id).cloned()
    }
    /// Creates missing history and returns an independent copy.
    pub fn get_or_create_rolling_store_stats(&self, id: u64) -> RollingStoreStats {
        self.rolling.write().unwrap().entry(id).or_default().clone()
    }
    pub fn observe(&self, id: u64, stats: &StoreStats) {
        self.rolling
            .write()
            .unwrap()
            .entry(id)
            .or_default()
            .observe(stats);
    }
    pub fn set(&self, id: u64, stats: &StoreStats) {
        self.rolling
            .write()
            .unwrap()
            .entry(id)
            .or_default()
            .set(stats);
    }
    /// Each tuple is (store ID, write bytes/second, write keys/second).
    pub fn observe_regions_stats(&self, rates: &[(u64, f64, f64)]) {
        let mut rolling = self.rolling.write().unwrap();
        for &(id, bytes, keys) in rates {
            rolling
                .entry(id)
                .or_default()
                .observe_regions_stats(bytes, keys);
        }
    }
    pub fn set_regions_stats(&self, rates: &[(u64, f64, f64)]) {
        let mut rolling = self.rolling.write().unwrap();
        for &(id, bytes, keys) in rates {
            rolling
                .entry(id)
                .or_default()
                .set_regions_stats(bytes, keys);
        }
    }
    pub fn get_store_read_cpu_recent_max(&self, id: u64) -> f64 {
        self.rolling
            .read()
            .unwrap()
            .get(&id)
            .map_or(0., RollingStoreStats::get_read_cpu_recent_max)
    }
    /// Filters using canonical core state and a host-supplied Unix nanosecond clock.
    pub fn filter_unhealthy_store(&self, stores: &StoresInfo, now_unix_nanos: i128) {
        self.rolling.write().unwrap().retain(|&id, _| {
            stores.get_store(id).is_some_and(|store| {
                !store.is_removed()
                    && !store.is_unhealthy(now_unix_nanos)
                    && !store.is_physically_destroyed()
            })
        });
    }
}
impl StoreStatInformer for StoresStats {
    fn get_stores_loads(&self) -> HashMap<u64, StoreKindLoads> {
        self.rolling
            .read()
            .unwrap()
            .iter()
            .map(|(&id, stats)| {
                (
                    id,
                    std::array::from_fn(|i| stats.get_load(StoreLoadKind::from_i32(i as i32))),
                )
            })
            .collect()
    }
}
