// Copyright 2026 AsterSQL.
use std::sync::{Arc, atomic::AtomicBool};

use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Base, MetaStore};

/// Default storage facade over the host's canonical metadata backend.
pub struct Storage {
    store: Arc<dyn MetaStore>,
    endpoint: StorageEndpoint,
    pub(crate) region_storage: Option<StorageEndpoint>,
    pub(crate) use_region_storage: AtomicBool,
    pub(crate) region_loaded: crate::region_load_once::RegionSource,
}

impl Storage {
    /// Assemble existing KV and endpoint capabilities over one supplied store.
    pub fn new(store: Arc<dyn MetaStore>) -> Self {
        Self {
            endpoint: StorageEndpoint::new(store.clone()),
            store,
            region_storage: None,
            use_region_storage: AtomicBool::new(false),
            region_loaded: crate::region_load_once::RegionSource::Unloaded,
        }
    }

    /// Access the existing KV API, including callback and conditional transactions.
    pub fn base(&self) -> Base<'_> {
        Base::new(self.store.as_ref())
    }

    /// Access existing persistence adapters without duplicating their behavior.
    pub fn endpoint(&self) -> &StorageEndpoint {
        &self.endpoint
    }

    /// Access adapters that maintain caller-owned region scan state.
    pub fn endpoint_mut(&mut self) -> &mut StorageEndpoint {
        &mut self.endpoint
    }
}
