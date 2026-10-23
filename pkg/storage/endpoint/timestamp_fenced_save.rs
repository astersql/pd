// Copyright 2026 AsterSQL.
use crate::StorageEndpoint;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, Compare, StoreError};
use astersql_pd_typeutil::BytesToU64Error;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// Leadership, monotonicity, decoding and commit failures remain distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum TimestampSaveError {
    #[error("leadership has not been granted")]
    NotLeader,
    #[error("saving timestamp is less than or equal to the previous one")]
    NotIncreasing,
    #[error(transparent)]
    Decode(#[from] BytesToU64Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("transaction conflict")]
    Conflict,
}
impl StorageEndpoint {
    /// Save once under caller-supplied leadership fencing conditions and a value CAS.
    /// The host must supply comparisons identifying its granted term (and lease if used).
    /// Empty conditions and empty expected leader values are rejected as ungranted.
    /// A failed fence or concurrent timestamp change returns Conflict; no retry occurs.
    /// Cancellation after dispatch can leave the commit outcome unknown.
    pub async fn save_timestamp(
        &self,
        paths: &ClusterPaths,
        group_id: u32,
        ts: SystemTime,
        conditions: Vec<Compare>,
        cancel: &CancellationToken,
    ) -> Result<(), TimestampSaveError> {
        if conditions.is_empty()
            || conditions
                .iter()
                .any(|c| matches!(c, Compare::Value(_, value) if value.is_empty()))
        {
            return Err(TimestampSaveError::NotLeader);
        }
        if cancel.is_cancelled() {
            return Err(StoreError::Cancelled.into());
        }
        let key = paths.timestamp_path(group_id);
        let base = Base::new(self.store.as_ref());
        let save = base.run_in_txn_with_conditions(conditions, async |txn| {
            let previous = txn.load(key.as_bytes()).await?;
            if !previous.is_empty() {
                let previous = astersql_pd_typeutil::parse_timestamp(&previous)?;
                if astersql_pd_typeutil::sub_real_time_by_wall_clock(ts, previous).as_nanos() <= 0 {
                    return Err(TimestampSaveError::NotIncreasing);
                }
            }
            let nanos =
                astersql_pd_typeutil::sub_real_time_by_wall_clock(ts, UNIX_EPOCH).as_nanos();
            txn.save(
                key.as_bytes(),
                &astersql_pd_typeutil::u64_to_bytes(nanos as u64),
            )?;
            Ok(())
        });
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StoreError::Unavailable.into()),
            result = save => result,
        };
        result.map_err(|error| match error {
            CallbackTxnError::Callback(error) => error,
            CallbackTxnError::Store(error) => error.into(),
            CallbackTxnError::Conflict => TimestampSaveError::Conflict,
        })
    }
}
