// Copyright 2026 AsterSQL.
use crate::{EndpointError, RegionMetaError, StorageEndpoint, region_meta::decode_region};
use astersql_pd_core::RegionInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Consistency, Read};
use tokio_util::sync::CancellationToken;

/// Region recovery failures preserve the endpoint's cursor and prior callbacks.
#[derive(Debug, thiserror::Error)]
pub enum RegionScanError {
    #[error("region scan cancelled")]
    Cancelled,
    #[error(transparent)]
    Region(#[from] RegionMetaError),
}
impl StorageEndpoint {
    /// Next metadata ID to recover, retained across calls, including failures.
    pub fn next_region_id(&self) -> u64 {
        self.next_region_id
    }
    /// Recover regions incrementally in ID order with Go's adaptive page size.
    /// The upper bound excludes MAX. Cancellation is checked after successful
    /// reads; an already delivered page is completed even when its callback cancels.
    /// Advance before callbacks and overlap deletion, matching Go's retry cursor.
    pub async fn load_regions(
        &mut self,
        paths: &ClusterPaths,
        cancel: &CancellationToken,
        mut put: impl FnMut(RegionInfo) -> Vec<RegionInfo>,
    ) -> Result<(), RegionScanError> {
        let end = paths.region_path(u64::MAX);
        let mut limit = 10000;
        loop {
            let start = paths.region_path(self.next_region_id);
            let mut request =
                Read::range(start.as_bytes(), end.as_bytes(), Consistency::Linearizable);
            request.limit = limit;
            let page = match self.store.read(request).await {
                Ok(page) => page,
                Err(error) => {
                    limit /= 2;
                    if limit >= 100 {
                        continue;
                    }
                    return Err(RegionMetaError::from(EndpointError::from(error)).into());
                }
            };
            if cancel.is_cancelled() {
                return Err(RegionScanError::Cancelled);
            }
            for entry in &page.entries {
                let meta = decode_region(&entry.value)?;
                self.next_region_id = meta.id.wrapping_add(1);
                for overlap in put(RegionInfo::new(meta, None, vec![], vec![])) {
                    self.delete_region(paths, overlap.meta()).await?;
                }
            }
            if page.entries.len() < limit {
                return Ok(());
            }
        }
    }
}
