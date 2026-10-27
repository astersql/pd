// Copyright 2026 AsterSQL.

use crate::kv::{Base, Consistency, Read};
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
impl StorageEndpoint {
    /// Load raw bytes; missing and stored empty values both return empty bytes.
    pub async fn load_scheduler_config(
        &self,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<Vec<u8>, EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .load(paths.scheduler_config_path(name).as_bytes())
            .await?)
    }
    /// Overwrite one scheduler key verbatim, without JSON encoding or a lease.
    pub async fn save_scheduler_config(
        &self,
        paths: &ClusterPaths,
        name: &str,
        data: &[u8],
    ) -> Result<(), EndpointError> {
        Base::new(self.store.as_ref())
            .save(paths.scheduler_config_path(name).as_bytes(), data)
            .await?;
        Ok(())
    }
    /// Remove exactly one scheduler key; removing a missing key succeeds.
    pub async fn remove_scheduler_config(
        &self,
        paths: &ClusterPaths,
        name: &str,
    ) -> Result<(), EndpointError> {
        Base::new(self.store.as_ref())
            .remove(paths.scheduler_config_path(name).as_bytes())
            .await?;
        Ok(())
    }
    /// Return up to 100 (name, raw configuration) pairs in bytewise key order.
    /// Names have the cluster prefix removed and remain bytes, preserving Go
    /// strings even when externally written keys contain invalid UTF-8.
    pub async fn load_all_scheduler_configs(
        &self,
        paths: &ClusterPaths,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, EndpointError> {
        let prefix = paths.scheduler_config_path_prefix();
        let mut request = Read::prefix(prefix.as_bytes(), Consistency::Linearizable);
        request.limit = 100; // Go MinKVRangeLimit: one read, no pagination.
        let snapshot = self.store.read(request).await?;
        Ok(snapshot
            .entries
            .into_iter()
            .map(|entry| (entry.key[prefix.len()..].to_vec(), entry.value))
            .collect())
    }
}
