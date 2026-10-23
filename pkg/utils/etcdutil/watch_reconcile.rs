// Copyright 2026 AsterSQL.
use std::{collections::BTreeSet, sync::Arc};

use astersql_pd_storage_kv::{Event, MetaStore, Result, Revision, StoreError, WatchBatch};
use tokio_util::sync::CancellationToken;

use crate::{LoadedSnapshot, SnapshotWatch};

/// Opt-in snapshot/watch recovery with a ledger of successfully published keys.
/// The host publishes the initial snapshot before consuming this observer.
/// Callbacks must publish the whole batch atomically, leaving no effects on error.
/// Errors return to the host, which owns retry timing and cancellation.
pub struct ReconciledWatch {
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    with_prefix: bool,
    batch_size: usize,
    watch: SnapshotWatch,
    keys: BTreeSet<Vec<u8>>,
    cancelled: bool,
    reload_required: bool,
}

impl ReconciledWatch {
    pub async fn load(
        store: Arc<dyn MetaStore>,
        key: Vec<u8>,
        with_prefix: bool,
        batch_size: usize,
        cancel: &CancellationToken,
    ) -> Result<(LoadedSnapshot, Self)> {
        let (snapshot, watch) =
            SnapshotWatch::load(store.clone(), key.clone(), with_prefix, batch_size, cancel)
                .await?;
        let keys = snapshot
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        Ok((
            snapshot,
            Self {
                store,
                key,
                with_prefix,
                batch_size,
                watch,
                keys,
                cancelled: false,
                reload_required: false,
            },
        ))
    }

    pub fn next_revision(&self) -> Revision {
        self.watch.next_revision()
    }

    /// Publish one live batch or a complete replacement after compaction.
    /// A failed replacement preserves both the old cursor and key ledger.
    /// Repeated reloads therefore emit a missing key only until its successful
    /// deletion is published. Dropping the observer releases its watch stream.
    pub async fn apply_next<F>(
        &mut self,
        cancel: &CancellationToken,
        mut apply: F,
    ) -> Result<Revision>
    where
        F: FnMut(&WatchBatch) -> Result<()>,
    {
        if self.cancelled || cancel.is_cancelled() {
            self.cancelled = true;
            let stopped = CancellationToken::new();
            stopped.cancel();
            return self.watch.apply_next(&stopped, |_| Ok(())).await;
        }
        let result = if self.reload_required {
            self.reload(cancel, &mut apply).await
        } else {
            let keys = &mut self.keys;
            let mut callback_invoked = false;
            let result = self
                .watch
                .apply_next(cancel, |batch| {
                    callback_invoked = true;
                    apply(batch)?;
                    for event in &batch.events {
                        match event {
                            Event::Put(entry) => {
                                keys.insert(entry.key.clone());
                            }
                            Event::Delete { key, .. } => {
                                keys.remove(key);
                            }
                        }
                    }
                    Ok(())
                })
                .await;
            if !callback_invoked && matches!(result, Err(StoreError::Compacted { .. })) {
                self.reload_required = true;
                self.reload(cancel, &mut apply).await
            } else {
                result
            }
        };
        if result == Err(StoreError::Cancelled) {
            self.cancelled = true;
        }
        result
    }

    async fn reload<F>(&mut self, cancel: &CancellationToken, apply: &mut F) -> Result<Revision>
    where
        F: FnMut(&WatchBatch) -> Result<()>,
    {
        let (snapshot, watch) = SnapshotWatch::load(
            self.store.clone(),
            self.key.clone(),
            self.with_prefix,
            self.batch_size,
            cancel,
        )
        .await?;
        let keys: BTreeSet<_> = snapshot
            .entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        let mut events: Vec<_> = snapshot.entries.into_iter().map(Event::Put).collect();
        events.extend(self.keys.difference(&keys).map(|key| Event::Delete {
            key: key.clone(),
            revision: snapshot.revision,
        }));
        apply(&WatchBatch {
            revision: snapshot.revision,
            events,
        })?;
        self.keys = keys;
        self.watch = watch;
        self.reload_required = false;
        Ok(self.next_revision())
    }
}
