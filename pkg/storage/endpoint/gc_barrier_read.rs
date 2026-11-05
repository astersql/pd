// Copyright 2026 AsterSQL.

use crate::kv::{Consistency, Read};
use crate::{
    EndpointError, GCBarrier, GlobalGCBarrier, LoadState, SafePointReadError, ServiceSafePoint,
    StorageEndpoint,
};
use astersql_pd_keypath::ClusterPaths;
use serde::de::DeserializeOwned;

impl StorageEndpoint {
    async fn load_barrier_json<T: DeserializeOwned>(
        &self,
        key: &[u8],
    ) -> Result<Option<T>, EndpointError> {
        match self.load(key).await? {
            LoadState::Missing | LoadState::Empty => Ok(None),
            LoadState::Value(bytes) => {
                serde_json::from_slice(&bytes).map_err(EndpointError::JsonUnmarshal)
            }
        }
    }

    async fn load_barrier_prefix<T: DeserializeOwned>(
        &self,
        prefix: &str,
    ) -> Result<(Vec<Vec<u8>>, Vec<Option<T>>), EndpointError> {
        let read = Read::prefix(prefix.as_bytes(), Consistency::Linearizable);
        let snapshot = self.store.read(read).await?;
        let mut keys = Vec::with_capacity(snapshot.entries.len());
        let mut values = Vec::with_capacity(snapshot.entries.len());
        for entry in snapshot.entries {
            values
                .push(serde_json::from_slice(&entry.value).map_err(EndpointError::JsonUnmarshal)?);
            keys.push(entry.key);
        }
        Ok((keys, values))
    }

    /// Loads a legacy service safe point as a barrier, preserving absence and JSON null.
    pub async fn load_gc_barrier(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
        id: &str,
    ) -> Result<Option<GCBarrier>, EndpointError> {
        self.load_gc_barrier_with_service_id(paths, keyspace_id, id.as_bytes())
            .await
    }
    /// Reads the exact byte-valued legacy service key and decodes its canonical JSON value.
    pub async fn load_gc_barrier_with_service_id(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
        id: &[u8],
    ) -> Result<Option<GCBarrier>, EndpointError> {
        let mut key = paths.gc_barrier_prefix(keyspace_id).into_bytes();
        key.extend_from_slice(id);
        Ok(self
            .load_barrier_json::<ServiceSafePoint>(&key)
            .await?
            .as_ref()
            .map(GCBarrier::from_service_safe_point))
    }

    /// Loads every barrier in key order, including reserved IDs, expired entries and nulls.
    pub async fn load_all_gc_barriers(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
    ) -> Result<Vec<Option<GCBarrier>>, EndpointError> {
        let (_, values) = self
            .compatible_load_all_service_gc_safe_points(paths, keyspace_id)
            .await?;
        Ok(values
            .iter()
            .map(|value| value.as_ref().map(GCBarrier::from_service_safe_point))
            .collect())
    }

    /// Loads one global barrier without applying expiration policy.
    pub async fn load_global_gc_barrier(
        &self,
        paths: &ClusterPaths,
        id: &str,
    ) -> Result<Option<GlobalGCBarrier>, EndpointError> {
        self.load_barrier_json(paths.global_gc_barrier_path(id).as_bytes())
            .await
    }

    /// Loads all global barriers with one unlimited prefix read.
    pub async fn load_all_global_gc_barriers(
        &self,
        paths: &ClusterPaths,
    ) -> Result<Vec<Option<GlobalGCBarrier>>, EndpointError> {
        Ok(self
            .load_barrier_prefix(&paths.global_gc_barrier_prefix())
            .await?
            .1)
    }

    /// Returns full physical keys and legacy values in matching order.
    /// Keys remain bytes so arbitrary stored identifiers are preserved without UTF-8 loss.
    pub async fn compatible_load_all_service_gc_safe_points(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
    ) -> Result<(Vec<Vec<u8>>, Vec<Option<ServiceSafePoint>>), EndpointError> {
        self.load_barrier_prefix(&paths.gc_barrier_prefix(keyspace_id))
            .await
    }

    /// Returns the identifier and smallest decimal min start TS; ties retain the first key.
    pub async fn compatible_load_tidb_min_start_ts(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
    ) -> Result<(Vec<u8>, u64), SafePointReadError> {
        let prefix = paths.compatible_tidb_min_start_ts_prefix(keyspace_id);
        let snapshot = self
            .store
            .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
            .await?;
        let mut minimum = None;
        for entry in snapshot.entries {
            let (value, error) = astersql_pd_typeutil::parse_uint64_with_base(
                &entry.value,
                astersql_pd_typeutil::GoUintBase::Decimal,
            );
            if let Some(error) = error {
                return Err(SafePointReadError::ParseUint(error));
            }
            if minimum.as_ref().is_none_or(|(_, old)| value < *old) {
                minimum = Some((entry.key[prefix.len()..].to_vec(), value));
            }
        }
        Ok(minimum.unwrap_or_default())
    }
}
