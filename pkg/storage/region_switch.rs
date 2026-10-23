// Copyright 2026 AsterSQL.
use std::sync::atomic::Ordering;

use astersql_pd_core::{RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{
    RegionLoadError, RegionMetaError, RegionScanError, StorageEndpoint,
};
use tokio_util::sync::CancellationToken;

use crate::Storage;

impl Storage {
    /// Attach a host-supplied region endpoint; all other metadata keeps its default source.
    /// The injected endpoint does not imply a production local persistence backend.
    pub fn new_core(mut default: Self, region_storage: StorageEndpoint) -> Self {
        default.region_storage = Some(region_storage);
        default.region_lifecycle = None;
        default.region_loaded = crate::region_load_once::RegionSource::Unloaded;
        default.use_region_storage.store(false, Ordering::SeqCst);
        default
    }

    /// Return the attached region endpoint regardless of the currently selected source.
    /// A plain facade returns its default endpoint, matching Go RetrieveRegionStorage.
    pub fn retrieve_region_storage(&self) -> &StorageEndpoint {
        self.region_storage.as_ref().unwrap_or(self.endpoint())
    }

    /// Select the region source and return it; a plain facade cannot switch.
    pub fn try_switch_region_storage(&self, use_local: bool) -> Option<&StorageEndpoint> {
        self.region_storage.as_ref()?;
        self.use_region_storage.store(use_local, Ordering::SeqCst);
        Some(if use_local {
            self.retrieve_region_storage()
        } else {
            self.endpoint()
        })
    }

    fn region_endpoint(&self) -> &StorageEndpoint {
        if self.use_region_storage.load(Ordering::SeqCst) {
            self.retrieve_region_storage()
        } else {
            self.endpoint()
        }
    }

    /// Load a region from the currently selected source, preserving existence errors.
    pub async fn load_region(
        &self,
        paths: &ClusterPaths,
        id: u64,
    ) -> Result<Option<RegionMeta>, RegionLoadError> {
        self.region_endpoint().load_region(paths, id).await
    }

    /// Save region metadata only to the selected source.
    pub async fn save_region(
        &self,
        paths: &ClusterPaths,
        region: &RegionMeta,
    ) -> Result<(), RegionMetaError> {
        self.region_endpoint().save_region(paths, region).await
    }

    /// Delete region metadata only from the selected source.
    pub async fn delete_region(
        &self,
        paths: &ClusterPaths,
        region: &RegionMeta,
    ) -> Result<(), RegionMetaError> {
        self.region_endpoint().delete_region(paths, region).await
    }

    /// Scan the selected endpoint, retaining that source's existing scan cursor.
    pub async fn load_regions(
        &mut self,
        paths: &ClusterPaths,
        cancel: &CancellationToken,
        put: impl FnMut(RegionInfo) -> Vec<RegionInfo>,
    ) -> Result<(), RegionScanError> {
        if self.use_region_storage.load(Ordering::SeqCst) {
            self.region_storage
                .as_mut()
                .expect("only a core facade can select region storage")
                .load_regions(paths, cancel, put)
                .await
        } else {
            self.endpoint_mut().load_regions(paths, cancel, put).await
        }
    }
}
