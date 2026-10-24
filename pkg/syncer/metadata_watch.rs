// Copyright 2026 AsterSQL.
use std::{collections::BTreeMap, sync::Arc};

use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_storage_kv::{Entry, Event, MetaStore, Result, Revision};
use tokio_util::sync::CancellationToken;

/// Host-driven metadata view; its revision never advances region history.
/// No worker or runtime is created. Drop releases the observation stream.
pub struct MetadataWatch {
    watch: ReconciledWatch,
    entries: BTreeMap<Vec<u8>, Entry>,
}
impl MetadataWatch {
    /// Publish a complete, revision-pinned prefix snapshot before observing changes.
    pub async fn load(
        store: Arc<dyn MetaStore>,
        prefix: Vec<u8>,
        page_size: usize,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let (snapshot, watch) =
            ReconciledWatch::load(store, prefix, true, page_size, cancel).await?;
        Ok(Self {
            watch,
            entries: snapshot
                .entries
                .into_iter()
                .map(|entry| (entry.key.clone(), entry))
                .collect(),
        })
    }

    pub fn entries(&self) -> &BTreeMap<Vec<u8>, Entry> {
        &self.entries
    }
    /// Inclusive resume position, distinct from HistoryIndex.
    pub fn next_revision(&self) -> Revision {
        self.watch.next_revision()
    }

    /// Apply one complete live batch or reconciled replacement after compaction.
    /// Unavailable preserves the view and cursor; the host chooses retry timing.
    /// Cancellation is terminal, including calls with a fresh token afterwards.
    pub async fn advance(&mut self, cancel: &CancellationToken) -> Result<Revision> {
        let entries = &mut self.entries;
        self.watch
            .apply_next(cancel, |batch| {
                // Exclusive access and no suspension/fallible publication inside the
                // callback keep the view and acknowledged cursor in one transition.
                for event in &batch.events {
                    match event {
                        Event::Put(entry) => {
                            entries.insert(entry.key.clone(), entry.clone());
                        }
                        Event::Delete { key, .. } => {
                            entries.remove(key);
                        }
                    }
                }
                Ok(())
            })
            .await
    }
}
