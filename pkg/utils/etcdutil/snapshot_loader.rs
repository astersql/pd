// Copyright 2026 AsterSQL.
use astersql_pd_storage_kv::{Consistency, Entry, MetaStore, Read, Result, Revision, StoreError};
use tokio_util::sync::CancellationToken;

/// A complete snapshot, ready for the caller to publish atomically.
#[derive(Debug)]
pub struct LoadedSnapshot {
    /// Revision selected by the first page, independent of later response headers.
    pub revision: Revision,
    /// Inclusive watch start immediately after this snapshot.
    pub next_revision: Revision,
    pub entries: Vec<Entry>,
}

/// Loads an exact key or prefix. Zero batch size means unlimited.
///
/// Always selects consistent loading: the Go legacy latest-page/callback mode is
/// not exposed by this API. The caller owns cancellation, store and publication.
/// All entries remain local until success; errors drop the staged snapshot.
/// No callbacks, detached workers or private runtime are installed.
pub struct SnapshotLoader {
    key: Vec<u8>,
    with_prefix: bool,
    batch_size: usize,
}
impl SnapshotLoader {
    pub fn new(key: Vec<u8>, with_prefix: bool, batch_size: usize) -> Self {
        Self {
            key,
            with_prefix,
            batch_size,
        }
    }

    /// Read all pages at the first response's revision, or return no snapshot.
    /// Empty prefix scans all keys; an empty exact key is invalid.
    pub async fn load(
        &self,
        store: &dyn MetaStore,
        cancel: &CancellationToken,
    ) -> Result<LoadedSnapshot> {
        if self.batch_size >= i64::MAX as usize || (!self.with_prefix && self.key.is_empty()) {
            return Err(StoreError::InvalidArgument);
        }
        let mut request = if self.with_prefix {
            Read::prefix(&self.key, Consistency::Linearizable)
        } else {
            Read::range(&self.key, b"", Consistency::Linearizable)
        };
        request.limit = if self.batch_size == 0 {
            0
        } else {
            self.batch_size + 1
        };
        let mut entries = Vec::new();
        loop {
            let mut page = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(StoreError::Cancelled),
                page = store.read(request.clone()) => page?,
            };
            if cancel.is_cancelled() {
                return Err(StoreError::Cancelled);
            }
            // Historical reads may return a newer header: never repin to it.
            let revision = *request.revision.get_or_insert(page.revision);
            if revision <= 0 {
                return Err(StoreError::Backend);
            }
            if page.more {
                // Go requests limit+1, reserving the final key as inclusive cursor.
                // Retain the original prefix rather than deriving it from cursor.
                if page.entries.len() < 2 || !self.with_prefix {
                    return Err(StoreError::Backend);
                }
                let cursor = page.entries.pop().unwrap().key;
                if request.start.as_ref().is_some_and(|start| cursor <= *start) {
                    return Err(StoreError::Backend);
                }
                request.start = Some(cursor);
            }
            entries.extend(page.entries);
            if !page.more {
                return Ok(LoadedSnapshot {
                    revision,
                    next_revision: revision.checked_add(1).ok_or(StoreError::Backend)?,
                    entries,
                });
            }
        }
    }
}
