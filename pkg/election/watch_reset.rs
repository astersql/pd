// Copyright 2026 AsterSQL.
use crate::{ElectionError, LeaderWatch, Leadership, Result};
use astersql_pd_storage_domain::*;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// An authoritative replacement snapshot or a subsequent exact-key event.
/// A snapshot after compaction replaces state; it does not reconstruct lost history.
#[derive(Debug)]
pub enum LeaderUpdate {
    Snapshot(Snapshot),
    Event(Event),
}

/// Snapshot followed by a gap-free watch, with explicit snapshots after compaction.
/// Uses the canonical LeaderWatch and the caller's runtime and cancellation token.
pub struct LeaderObserver {
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    watch: Option<LeaderWatch>,
    cancelled: bool,
}
impl LeaderObserver {
    pub fn new(store: Arc<dyn MetaStore>, key: Vec<u8>) -> Result<Self> {
        if key.is_empty() {
            return Err(ElectionError::InvalidArgument);
        }
        Ok(Self {
            store,
            key,
            watch: None,
            cancelled: false,
        })
    }

    /// Deliver a linearizable snapshot before watching its successor revision.
    /// Delete remains terminal until the caller starts a new observer.
    pub async fn next(&mut self, cancel: &CancellationToken) -> Result<LeaderUpdate> {
        if self.cancelled || cancel.is_cancelled() {
            self.cancelled = true;
            self.watch = None;
            return Err(ElectionError::Cancelled);
        }
        if let Some(watch) = &mut self.watch {
            match watch.next(cancel).await {
                Ok(event) => return Ok(LeaderUpdate::Event(event)),
                Err(ElectionError::Store(StoreError::Compacted { .. })) => {
                    // History is lost: replace the observed state using a fresh
                    // linearizable read, never jump directly to the compact boundary.
                    self.watch = None;
                }
                Err(ElectionError::Cancelled) => {
                    self.cancelled = true;
                    self.watch = None;
                    return Err(ElectionError::Cancelled);
                }
                Err(error) => return Err(error),
            }
        }
        let snapshot = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                self.cancelled = true;
                return Err(ElectionError::Cancelled);
            }
            result = self.store.read(Read::range(&self.key, b"", Consistency::Linearizable)) => result?,
        };
        self.watch = Some(LeaderWatch::new(
            self.store.clone(),
            self.key.clone(),
            snapshot
                .revision
                .checked_add(1)
                .ok_or(StoreError::Backend)?,
        )?);
        Ok(LeaderUpdate::Snapshot(snapshot))
    }
}

/// Publication slot for immutable terms; cleanup always captures one specific term.
/// The host publishes only successful campaigns for the same election key here.
#[derive(Default)]
pub struct LeadershipSlot {
    current: Mutex<Option<Arc<Leadership>>>,
}
impl LeadershipSlot {
    pub fn publish(&self, term: Arc<Leadership>) {
        *self.current.lock().unwrap() = Some(term);
    }
    pub fn current(&self) -> Option<Arc<Leadership>> {
        self.current.lock().unwrap().clone()
    }
    /// Reset the captured term without closing a replacement's lease or renewal loop.
    /// Cleanup is allowed after host cancellation, like Leadership::resign.
    pub async fn reset(&self, term: &Arc<Leadership>) -> Result<()> {
        {
            let mut current = self.current.lock().unwrap();
            if current
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, term))
            {
                current.take();
            }
        }
        // Never hold the publication mutex across backend IO, and never touch
        // the slot after awaiting: a replacement may have been published meanwhile.
        term.resign().await
    }
}
