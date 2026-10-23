// Copyright 2026 AsterSQL.
use crate::StorageEndpoint;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, StoreError};
use astersql_pd_typeutil::BytesToU64Error;
use std::time::SystemTime;
use tokio_util::sync::CancellationToken;

/// Timestamp decoding and backend failures remain separate.
#[derive(Debug, thiserror::Error)]
pub enum TimestampWindowError {
    #[error(transparent)]
    Decode(#[from] BytesToU64Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("transaction conflict")]
    Conflict,
}
impl StorageEndpoint {
    /// Read a Go timestamp window. None represents Go's zero time (year 1),
    /// distinct from a stored eight-byte zero representing the Unix epoch.
    pub async fn load_timestamp(
        &self,
        paths: &ClusterPaths,
        group_id: u32,
    ) -> Result<Option<SystemTime>, TimestampWindowError> {
        let bytes = Base::new(self.store.as_ref())
            .load(paths.timestamp_path(group_id).as_bytes())
            .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(astersql_pd_typeutil::parse_timestamp(&bytes)?))
    }

    /// Delete exactly one group's window in a callback transaction.
    /// Cancellation after dispatch may leave the commit outcome unknown.
    pub async fn delete_timestamp(
        &self,
        paths: &ClusterPaths,
        group_id: u32,
        cancel: &CancellationToken,
    ) -> Result<(), TimestampWindowError> {
        if cancel.is_cancelled() {
            return Err(StoreError::Cancelled.into());
        }
        let key = paths.timestamp_path(group_id);
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StoreError::Unavailable.into()),
            result = self.run_in_txn(async |txn| txn.remove(key.as_bytes())) => result,
        };
        result.map_err(|error| match error {
            CallbackTxnError::Callback(error) | CallbackTxnError::Store(error) => error.into(),
            CallbackTxnError::Conflict => TimestampWindowError::Conflict,
        })
    }
}
