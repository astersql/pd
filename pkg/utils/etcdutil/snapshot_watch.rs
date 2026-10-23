// Copyright 2026 AsterSQL.
use std::sync::Arc;

use astersql_pd_storage_kv::{MetaStore, MetaWatch, Result, Revision, StoreError, WatchBatch};
use tokio_util::sync::CancellationToken;

use crate::{LoadedSnapshot, SnapshotLoader};

/// Pull-based continuation of a complete consistent snapshot.
///
/// The host publishes the returned snapshot before calling `apply_next`. No
/// worker or runtime is created. The host controls retry delays after errors;
/// dropping this object closes its stream. Compaction requires a new snapshot.
pub struct SnapshotWatch {
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    with_prefix: bool,
    next_revision: Revision,
    watch: Option<Box<dyn MetaWatch>>,
    pending: Option<WatchBatch>,
    cancelled: bool,
}

impl SnapshotWatch {
    pub async fn load(
        store: Arc<dyn MetaStore>,
        key: Vec<u8>,
        with_prefix: bool,
        batch_size: usize,
        cancel: &CancellationToken,
    ) -> Result<(LoadedSnapshot, Self)> {
        let snapshot = SnapshotLoader::new(key.clone(), with_prefix, batch_size)
            .load(store.as_ref(), cancel)
            .await?;
        let next_revision = snapshot.next_revision;
        Ok((
            snapshot,
            Self {
                store,
                key,
                with_prefix,
                next_revision,
                watch: None,
                pending: None,
                cancelled: false,
            },
        ))
    }

    /// Inclusive resume position of the last successfully published batch.
    pub fn next_revision(&self) -> Revision {
        self.next_revision
    }

    /// Apply a batch synchronously; the callback must publish atomically or be
    /// replay-safe. Errors are returned to the host, which chooses when to retry.
    /// Unavailable closes the stream; the next call reconnects at the unacknowledged
    /// revision. Callback errors retain the complete batch for retry.
    /// Cancellation is terminal, even if a later call uses a different token.
    pub async fn apply_next<F>(&mut self, cancel: &CancellationToken, apply: F) -> Result<Revision>
    where
        F: FnOnce(&WatchBatch) -> Result<()>,
    {
        if self.cancelled || cancel.is_cancelled() {
            self.cancelled = true;
            self.watch = None;
            self.pending = None;
            return Err(StoreError::Cancelled);
        }
        loop {
            if self.pending.is_none() {
                if self.watch.is_none() {
                    self.watch = Some(tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {
                            self.cancelled = true;
                            return Err(StoreError::Cancelled);
                        },
                        watch = self.store.watch(self.key.clone(), self.next_revision) => watch?,
                    });
                }
                let result = self.watch.as_mut().unwrap().next(cancel).await;
                match result {
                    Ok(mut batch) => {
                        if !self.with_prefix {
                            batch.events.retain(|event| event.key() == self.key);
                        }
                        self.pending = Some(batch);
                    }
                    Err(error) => {
                        self.watch = None;
                        self.cancelled = error == StoreError::Cancelled;
                        return Err(error);
                    }
                }
            }
            let batch = self.pending.as_ref().unwrap();
            let next = batch.revision.checked_add(1).ok_or(StoreError::Backend)?;
            if cancel.is_cancelled() {
                self.cancelled = true;
                self.watch = None;
                self.pending = None;
                return Err(StoreError::Cancelled);
            }
            if batch.events.is_empty() {
                self.next_revision = self.next_revision.max(next);
                self.pending = None;
                continue;
            }
            apply(batch)?;
            self.next_revision = self.next_revision.max(next);
            self.pending = None;
            return Ok(self.next_revision);
        }
    }
}
