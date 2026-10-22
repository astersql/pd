// Copyright 2026 AsterSQL.
//! Store registry. Membership and copy-on-write patches share one lock.
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{StoreInfo, StoreMeta};

/// Thread-safe registry retaining immutable store handles for snapshots.
#[derive(Default)]
pub struct StoresInfo {
    pub(crate) stores: RwLock<HashMap<u64, Arc<StoreInfo>>>,
}
impl StoresInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_store(&self, id: u64) -> Option<Arc<StoreInfo>> {
        self.stores.read().unwrap().get(&id).cloned()
    }

    /// Snapshot membership; ordering is unspecified, as with Go map iteration.
    pub fn get_stores(&self) -> Vec<Arc<StoreInfo>> {
        self.stores.read().unwrap().values().cloned().collect()
    }

    /// Preserve absent metadata for stores constructed without metadata.
    pub fn get_meta_stores(&self) -> Vec<Option<Arc<StoreMeta>>> {
        self.stores
            .read()
            .unwrap()
            .values()
            .map(|s| s.meta.clone())
            .collect()
    }

    pub fn get_store_ids(&self) -> Vec<u64> {
        self.stores
            .read()
            .unwrap()
            .values()
            .map(|s| s.id())
            .collect()
    }

    pub fn store_count(&self) -> usize {
        self.stores.read().unwrap().len()
    }

    pub fn put_store(&self, store: Arc<StoreInfo>) {
        self.stores.write().unwrap().insert(store.id(), store);
    }

    /// Apply options to the current entry under the write lock. Missing entries
    /// are skipped, so an in-flight patch cannot resurrect a deleted store.
    /// The callback must not re-enter this registry or panic.
    pub fn patch_store(&self, id: u64, patch: impl FnOnce(&mut StoreInfo)) -> bool {
        let mut stores = self.stores.write().unwrap();
        let Some(current) = stores.get(&id) else {
            return false;
        };
        let mut next = current.as_ref().clone();
        patch(&mut next);
        stores.insert(id, Arc::new(next));
        true
    }

    pub fn delete_store(&self, id: u64) {
        self.stores.write().unwrap().remove(&id);
    }

    pub fn reset_stores(&self) {
        self.stores.write().unwrap().clear();
    }
}
