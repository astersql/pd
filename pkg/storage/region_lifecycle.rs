// Copyright 2026 AsterSQL.
use std::{error::Error, sync::Arc};

use astersql_pd_storage_endpoint::StorageEndpoint;

use crate::Storage;

/// The original error returned by a host-supplied region IO lifecycle.
pub type RegionLifecycleError = Box<dyn Error + Send + Sync>;

/// Lifecycle of an external region storage IO boundary, owned by the host.
/// Implementations decide retry and repeated-close behavior; the facade forwards every call.
pub trait RegionLifecycle: Send + Sync {
    /// Flush dirty regions to the external storage.
    fn flush(&self) -> Result<(), RegionLifecycleError>;
    /// Close the external region storage.
    fn close(&self) -> Result<(), RegionLifecycleError>;
}

impl Storage {
    /// Attach a region endpoint together with its host-provided IO lifecycle.
    /// The ordinary `new_core` endpoint has Go StorageEndpoint's no-op lifecycle.
    pub fn new_core_with_lifecycle(
        default: Self,
        region_storage: StorageEndpoint,
        lifecycle: Arc<dyn RegionLifecycle>,
    ) -> Self {
        let mut storage = Self::new_core(default, region_storage);
        storage.region_lifecycle = Some(lifecycle);
        storage
    }

    /// Flush the attached region storage, independently of the selected read source.
    pub fn flush(&self) -> Result<(), RegionLifecycleError> {
        match &self.region_lifecycle {
            Some(lifecycle) => lifecycle.flush(),
            None => Ok(()),
        }
    }

    /// Close only the attached region storage; leave the default metadata backend owned by the host.
    pub fn close(&self) -> Result<(), RegionLifecycleError> {
        match &self.region_lifecycle {
            Some(lifecycle) => lifecycle.close(),
            None => Ok(()),
        }
    }
}
