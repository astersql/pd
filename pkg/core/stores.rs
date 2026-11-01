// Copyright 2026 AsterSQL.
//! Store registry. Membership and copy-on-write patches share one lock.
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{StoreInfo, StoreMeta};

/// Thread-safe registry retaining immutable store handles for snapshots.
#[derive(Clone, Default)]
pub struct StoresInfo {
    filter_metrics: Arc<RwLock<Option<Arc<dyn crate::metrics::StoreMetricSink>>>>,
    pub(crate) stores: Arc<RwLock<HashMap<u64, Arc<StoreInfo>>>>,
}
impl StoresInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_filter_metrics(&self, sink: Arc<dyn crate::metrics::StoreMetricSink>) {
        let stores = self.stores.write().unwrap();
        let sink: Arc<dyn crate::metrics::StoreMetricSink> =
            Arc::new(crate::metrics::LiveStoreMetrics {
                stores: Arc::downgrade(&self.stores),
                sink,
            });
        *self.filter_metrics.write().unwrap() = Some(sink.clone());
        for store in stores.values() {
            *store.filter_metrics.write().unwrap() = Some(sink.clone());
        }
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
        let mut stores = self.stores.write().unwrap();
        if let Some(metrics) = self.filter_metrics.read().unwrap().clone() {
            *store.filter_metrics.write().unwrap() = Some(metrics.clone());
            if store.is_removed() {
                metrics.delete_store_metrics(store.id());
            }
        }
        stores.insert(store.id(), store);
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
        if next.is_removed() {
            if let Some(metrics) = self.filter_metrics.read().unwrap().as_ref() {
                metrics.delete_store_metrics(id);
            }
        }
        stores.insert(id, Arc::new(next));
        true
    }

    pub fn delete_store(&self, id: u64) {
        let mut stores = self.stores.write().unwrap();
        stores.remove(&id);
        if let Some(metrics) = self.filter_metrics.read().unwrap().as_ref() {
            metrics.delete_store_metrics(id);
        }
    }

    pub fn reset_stores(&self) {
        let mut stores = self.stores.write().unwrap();
        if let Some(metrics) = self.filter_metrics.read().unwrap().as_ref() {
            for id in stores.keys() {
                metrics.delete_store_metrics(*id);
            }
        }
        stores.clear();
    }
}
