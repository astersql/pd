// Copyright 2026 AsterSQL.
//! Clone boundaries: owned metadata for Clone, shared metadata for ShallowClone,
//! and shared heartbeat statistics, smoothing history and limiter for both.
use std::sync::{Arc, RwLock};

use crate::{StoreInfo, StoreStats};

#[derive(Debug, Default)]
pub(crate) struct SharedStoreStats {
    pub(crate) raw: StoreStats,
    pub(crate) average: Option<astersql_pd_movingaverage::Hma>,
}

impl Clone for StoreInfo {
    fn clone(&self) -> Self {
        let mut copy = self.shallow_clone();
        copy.meta = self.meta.as_deref().cloned().map(Arc::new);
        copy
    }
}

impl StoreInfo {
    /// Share metadata until a metadata setter replaces it. Safe metadata access
    /// is read-only; setters detach as Go's StoreCreateOptions do.
    pub fn shallow_clone(&self) -> Self {
        Self {
            last_persist_unix_nanos: self.last_persist_unix_nanos,
            last_awaken_unix_nanos: self.last_awaken_unix_nanos,
            meta: self.meta.clone(),
            stats: Arc::clone(&self.stats),
            limiter: self.limiter.clone(),
            leader_weight: self.leader_weight,
            region_weight: self.region_weight,
        }
    }

    /// The same limiter handle is retained by both kinds of store clone.
    /// A zero-value store has no limiter. Callers must release lock guards before
    /// using another handle to the same store's limiter.
    pub fn limiter(&self) -> Option<&Arc<RwLock<astersql_pd_storelimit::StoreRateLimit>>> {
        self.limiter.as_ref()
    }
}
