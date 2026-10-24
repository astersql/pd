// Copyright 2026 AsterSQL.
//! Next-index checkpoints; region records are deliberately never serialized.
use crate::{HistoryBuffer, HistoryError, HistoryIndex};
use astersql_pd_core::RegionInfo;
use astersql_pd_storage_kv::{Base, MetaStore, StoreError};
use std::sync::Arc;

/// Go's Base-relative checkpoint key. Hosts supply a suitably scoped MetaStore.
pub const HISTORY_KEY: &[u8] = b"historyIndex";
const FLUSH_COUNT: usize = 100;

/// Persistence failures return to the host instead of terminating its process.
#[derive(Debug, thiserror::Error)]
pub enum HistoryPersistError {
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("load history index failed: {0}")]
    Load(#[source] StoreError),
    #[error("invalid persisted history index")]
    CorruptIndex,
    /// Memory already contains this next index; retry persist(), not record().
    #[error("persist history index {next:?} failed: {source}")]
    Save {
        next: HistoryIndex,
        #[source]
        source: StoreError,
    },
}

/// Exclusively owned history plus a borrowed backend; no worker or private runtime.
/// Like Go, automatic writes occur every 100 records, even after a failed write.
/// A cancelled write can have committed: the host may retry the same checkpoint
/// with persist(), but must not replay record() or assume a failed reset rolled back.
/// Hosts must serialize writers for this key, including checkpoint retries.
pub struct PersistentHistory<'a> {
    history: HistoryBuffer,
    base: Base<'a>,
    remaining: usize,
}

impl<'a> PersistentHistory<'a> {
    /// Load only nextIndex into empty history, clearing any supplied records/retains.
    /// Missing/empty values mean zero. Load errors are surfaced for host policy.
    pub async fn load(
        mut history: HistoryBuffer,
        store: &'a dyn MetaStore,
    ) -> Result<Self, HistoryPersistError> {
        let base = Base::new(store);
        let value = base
            .load(HISTORY_KEY)
            .await
            .map_err(HistoryPersistError::Load)?;
        let next = if value.is_empty() {
            0
        } else {
            // ParseUint(base=10, bitSize=64) accepts ASCII digits only.
            if !value.iter().all(u8::is_ascii_digit) {
                return Err(HistoryPersistError::CorruptIndex);
            }
            std::str::from_utf8(&value)
                .map_err(|_| HistoryPersistError::CorruptIndex)?
                .parse::<u64>()
                .map_err(|_| HistoryPersistError::CorruptIndex)?
        };
        history.reset_with_index(HistoryIndex(next));
        Ok(Self {
            history,
            base,
            remaining: FLUSH_COUNT,
        })
    }

    /// Read the canonical replay window. No records are synthesized by reload.
    pub fn history(&self) -> &HistoryBuffer {
        &self.history
    }

    /// Assign one record, then checkpoint at the 100-record boundary.
    /// Save failure leaves the assigned record and index in memory.
    pub async fn record(
        &mut self,
        region: Arc<RegionInfo>,
    ) -> Result<HistoryIndex, HistoryPersistError> {
        let assigned = self.history.record(region)?;
        self.remaining -= 1;
        if self.remaining == 0 {
            self.remaining = FLUSH_COUNT;
            self.persist().await?;
        }
        Ok(assigned)
    }

    /// Explicit checkpoint does not restart the automatic flush countdown.
    pub async fn persist(&mut self) -> Result<(), HistoryPersistError> {
        let next = self.history.next_index();
        if let Some(metrics) = &self.history.metrics {
            metrics.persist_metrics(self.history.first_index(), next);
        }
        self.base
            .save(HISTORY_KEY, next.0.to_string().as_bytes())
            .await
            .map_err(|source| HistoryPersistError::Save { next, source })
    }

    /// Reset memory and its countdown first, then save the new next index.
    pub async fn reset_with_index_and_persist(
        &mut self,
        index: HistoryIndex,
    ) -> Result<(), HistoryPersistError> {
        self.reset_with_index(index);
        self.persist().await
    }

    /// Reset without I/O, matching Go resetWithIndex.
    pub fn reset_with_index(&mut self, index: HistoryIndex) {
        self.history.reset_with_index(index);
        self.remaining = FLUSH_COUNT;
    }

    /// Keep the existing adaptive replay-window implementation available to hosts.
    pub fn retain_from(&self, index: HistoryIndex) -> crate::HistoryRetain {
        self.history.retain_from(index)
    }
    /// Observe downstream replay demand without changing persistence cadence.
    pub fn observe_required_window(&mut self, window: u64) -> Result<(), HistoryError> {
        self.history.observe_required_window(window)
    }
    /// Apply the existing low-window shrink policy.
    pub fn maybe_shrink(&mut self) -> Result<(), HistoryError> {
        self.history.maybe_shrink()
    }
}
