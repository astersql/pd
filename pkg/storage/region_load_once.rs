// Copyright 2026 AsterSQL.
use std::sync::atomic::Ordering;

use astersql_pd_core::RegionInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::RegionScanError;
use tokio_util::sync::CancellationToken;

use crate::Storage;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionSource {
    Unloaded,
    Default,
    Local,
}

impl Storage {
    /// Load regions, skipping after any successful facade load when local is selected.
    /// Default loads always scan and overwrite the recorded source on success.
    /// Failures retain the previous source and the endpoint's retry cursor.
    /// The exclusive borrow serializes scanning and state updates across the await.
    pub async fn try_load_regions_once(
        &mut self,
        paths: &ClusterPaths,
        cancel: &CancellationToken,
        put: impl FnMut(RegionInfo) -> Vec<RegionInfo>,
    ) -> Result<(), RegionScanError> {
        if self.region_storage.is_none() {
            return self.load_regions(paths, cancel, put).await;
        }
        if !self.use_region_storage.load(Ordering::SeqCst) {
            self.endpoint_mut().load_regions(paths, cancel, put).await?;
            self.region_loaded = RegionSource::Default;
        } else if self.region_loaded == RegionSource::Unloaded {
            self.region_storage
                .as_mut()
                .expect("core facade has a region endpoint")
                .load_regions(paths, cancel, put)
                .await?;
            self.region_loaded = RegionSource::Local;
        }
        Ok(())
    }

    /// Report whether the selected source supplied the last successful load.
    /// Like Go AreRegionsLoaded, this requires a core facade.
    pub fn are_regions_loaded(&self) -> bool {
        assert!(
            self.region_storage.is_some(),
            "requires a core storage facade"
        );
        self.region_loaded
            == if self.use_region_storage.load(Ordering::SeqCst) {
                RegionSource::Local
            } else {
                RegionSource::Default
            }
    }
}
