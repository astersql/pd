// Copyright 2026 AsterSQL.
use crate::StorageEndpoint;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, StoreError};

/// Timestamp syntax/range errors remain distinct from backend failures.
#[derive(Debug, thiserror::Error)]
pub enum TimestampValueError {
    #[error("parse uint error")]
    ParseUint,
    #[error(transparent)]
    Store(#[from] StoreError),
}
impl StorageEndpoint {
    /// Read the minimum resolved timestamp; absent and empty values yield zero.
    pub async fn load_min_resolved_ts(
        &self,
        paths: &ClusterPaths,
    ) -> Result<u64, TimestampValueError> {
        self.load_timestamp_value(&paths.min_resolved_ts_path())
            .await
    }
    /// Persist the minimum resolved timestamp as lowercase hexadecimal without a lease.
    pub async fn save_min_resolved_ts(
        &self,
        paths: &ClusterPaths,
        value: u64,
    ) -> Result<(), TimestampValueError> {
        self.save_timestamp_value(&paths.min_resolved_ts_path(), value)
            .await
    }
    /// Read the external timestamp; absent and empty values yield zero.
    pub async fn load_external_ts(&self, paths: &ClusterPaths) -> Result<u64, TimestampValueError> {
        self.load_timestamp_value(&paths.external_timestamp_path())
            .await
    }
    /// Persist the external timestamp as lowercase hexadecimal without a lease.
    pub async fn save_external_ts(
        &self,
        paths: &ClusterPaths,
        value: u64,
    ) -> Result<(), TimestampValueError> {
        self.save_timestamp_value(&paths.external_timestamp_path(), value)
            .await
    }
    async fn save_timestamp_value(&self, key: &str, value: u64) -> Result<(), TimestampValueError> {
        Base::new(self.store.as_ref())
            .save(key.as_bytes(), format!("{value:x}").as_bytes())
            .await?;
        Ok(())
    }
    async fn load_timestamp_value(&self, key: &str) -> Result<u64, TimestampValueError> {
        let bytes = Base::new(self.store.as_ref()).load(key.as_bytes()).await?;
        if bytes.is_empty() {
            return Ok(0);
        }
        // Go ParseUint with an explicit base rejects signs, prefixes,
        // underscores and whitespace; Rust's parser alone accepts a plus sign.
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(TimestampValueError::ParseUint);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| TimestampValueError::ParseUint)?;
        u64::from_str_radix(text, 16).map_err(|_| TimestampValueError::ParseUint)
    }
}
