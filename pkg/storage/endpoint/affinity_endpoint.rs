// Copyright 2026 AsterSQL.
use crate::kv::{Base, CallbackTxn};
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::ClusterPaths;
use serde::Serialize;

impl StorageEndpoint {
    /// Load raw JSON bytes; missing and empty values both return empty bytes.
    pub async fn load_affinity_group(
        &self,
        paths: &ClusterPaths,
        group_id: &str,
    ) -> Result<Vec<u8>, EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .load(paths.affinity_group_path(group_id).as_bytes())
            .await?)
    }
    /// Encode a caller-owned model with Go-compatible field names and JSON encoding.
    /// The write participates in the caller's transaction.
    pub fn save_affinity_group<T: Serialize + ?Sized>(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        group_id: &str,
        group: &T,
    ) -> Result<(), EndpointError> {
        let bytes = serde_json::to_vec(group).map_err(EndpointError::JsonMarshal)?;
        Ok(txn.save(paths.affinity_group_path(group_id).as_bytes(), &bytes)?)
    }
    /// Buffer deletion of exactly one group in the caller's transaction.
    pub fn delete_affinity_group(
        &self,
        txn: &mut CallbackTxn<'_>,
        paths: &ClusterPaths,
        group_id: &str,
    ) -> Result<(), EndpointError> {
        Ok(txn.remove(paths.affinity_group_path(group_id).as_bytes())?)
    }
    /// Deliver raw suffix/value pairs in byte order, without decoding JSON.
    pub async fn load_all_affinity_groups(
        &self,
        paths: &ClusterPaths,
        callback: impl FnMut(&[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(paths.affinity_groups_prefix().as_bytes(), callback)
            .await
    }
}
