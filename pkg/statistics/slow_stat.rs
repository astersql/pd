// Copyright 2026 AsterSQL.
//! Synchronized slow-store membership with Go-compatible observation transitions.
use std::{collections::HashSet, ops::Deref, sync::RwLock};

/// Cached slow-store membership. Each transition holds one exclusive lock.
#[derive(Default)]
pub struct SlowStoresStats {
    slow_stores: RwLock<HashSet<u64>>,
}
impl SlowStoresStats {
    pub fn new() -> Self {
        Self::default()
    }
    /// Records an absent slow store; observing an existing store removes it.
    /// This deliberately preserves the non-idempotent Go transition.
    pub fn observe_slow_store_status(&self, store_id: u64, is_slow: bool) {
        let mut stores = self.slow_stores.write().unwrap();
        if is_slow && !stores.contains(&store_id) {
            stores.insert(store_id);
        } else {
            stores.remove(&store_id);
        }
    }
    /// Removes a store, including when it is already absent.
    pub fn remove_slow_store_status(&self, store_id: u64) {
        self.slow_stores.write().unwrap().remove(&store_id);
    }
    /// Returns whether at least one slow store is recorded.
    pub fn exists_slow_stores(&self) -> bool {
        !self.slow_stores.read().unwrap().is_empty()
    }
}
/// Cluster slow-node statistics container; dereferences to its shared cache.
#[derive(Default)]
pub struct SlowStat {
    pub slow_stores_stats: SlowStoresStats,
}
impl SlowStat {
    pub fn new() -> Self {
        Self::default()
    }
}
impl Deref for SlowStat {
    type Target = SlowStoresStats;
    fn deref(&self) -> &Self::Target {
        &self.slow_stores_stats
    }
}
