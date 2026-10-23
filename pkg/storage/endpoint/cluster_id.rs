// Copyright 2026 AsterSQL.
use crate::StorageEndpoint;
use astersql_pd_keypath::CLUSTER_ID_PATH;
use astersql_pd_storage_kv::{Compare, Consistency, Mutation, RawTxnOp, Read, StoreError};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const KEY: &[u8] = CLUSTER_ID_PATH.as_bytes();
const RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// Cluster identity decoding and transaction failures, separate from backend errors.
#[derive(Debug, thiserror::Error)]
pub enum ClusterIdError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("invalid cluster id byte length: {0}")]
    InvalidLength(usize),
    #[error("cluster id transaction conflict")]
    Conflict,
}

/// Go's `(uint64(unix_seconds) << 32) + uint64(random)` with wrapping arithmetic.
/// The host supplies time and randomness; generating zero is permitted by Go.
pub const fn cluster_id_candidate(unix_seconds: u64, random: u32) -> u64 {
    (unix_seconds << 32).wrapping_add(random as u64)
}

fn decode(bytes: &[u8]) -> Result<u64, ClusterIdError> {
    let bytes = bytes
        .try_into()
        .map_err(|_| ClusterIdError::InvalidLength(bytes.len()))?;
    Ok(u64::from_be_bytes(bytes))
}

impl StorageEndpoint {
    /// Read the stored big-endian ID; missing is zero, malformed values are errors.
    /// There is no process-global cache; each endpoint uses its host's store.
    pub async fn get_cluster_id(&self) -> Result<u64, ClusterIdError> {
        let snapshot = self
            .store
            .read(Read::range(KEY, b"", Consistency::Linearizable))
            .await?;
        match snapshot.entries.as_slice() {
            [] => Ok(0),
            [entry] => decode(&entry.value),
            _ => Err(ClusterIdError::Conflict),
        }
    }

    /// Return an existing ID, or atomically create one using host-supplied inputs.
    /// A losing initializer reads the winner in the same transaction. Backend
    /// errors are propagated without retrying an uncertain mutation outcome.
    pub async fn init_cluster_id(&self, seconds: u64, random: u32) -> Result<u64, ClusterIdError> {
        let existing = self.get_cluster_id().await?;
        if existing != 0 {
            return Ok(existing);
        }
        self.init_or_get_cluster_id(cluster_id_candidate(seconds, random))
            .await
    }

    pub(super) async fn init_or_get_cluster_id(
        &self,
        candidate: u64,
    ) -> Result<u64, ClusterIdError> {
        let response = self
            .store
            .transact_raw(
                vec![Compare::CreateRevision(KEY.to_vec(), 0)],
                vec![Mutation::put(KEY, &candidate.to_be_bytes()).into()],
                vec![RawTxnOp::Get(KEY.to_vec())],
            )
            .await?;
        if response.succeeded {
            return Ok(candidate);
        }
        match response.responses.as_slice() {
            [item] => match item.key_value_pairs.as_slice() {
                [pair] if pair.key == KEY => decode(&pair.value),
                _ => Err(ClusterIdError::Conflict),
            },
            _ => Err(ClusterIdError::Conflict),
        }
    }

    /// MCS only reads: retry missing, zero, malformed and unavailable values.
    /// Cancellation interrupts both an in-flight read and the retry delay,
    /// returning StoreError::Cancelled. The caller owns the future and runtime.
    pub async fn wait_cluster_id(&self, cancel: &CancellationToken) -> Result<u64, ClusterIdError> {
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + RETRY_INTERVAL, RETRY_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(StoreError::Cancelled.into()),
                result = self.get_cluster_id() => result,
            };
            if let Ok(id) = result
                && id != 0
            {
                return Ok(id);
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(StoreError::Cancelled.into()),
                _ = ticker.tick() => {},
            }
        }
    }
}
