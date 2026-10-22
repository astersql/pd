// Copyright 2026 AsterSQL.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::blocking::Mutex;

type HashFn = dyn Fn(u32) -> u32 + Send + Sync;

/// Serializes work sharing the same hashed ID while leaving other IDs independent.
pub struct LockGroup {
    entries: Mutex<HashMap<u32, Arc<AsyncMutex<()>>>>,
    hash_fn: Arc<HashFn>,
}

impl LockGroup {
    /// Creates a lock group that uses each ID as its own lock key.
    pub fn new() -> Self {
        Self::with_hash(|id| id)
    }

    /// Creates a lock group with a custom mapping from IDs to lock keys.
    ///
    /// IDs producing the same hash share one lock. This can be used to bound
    /// the number of locks retained by the group.
    pub fn with_hash<F>(hash_fn: F) -> Self
    where
        F: Fn(u32) -> u32 + Send + Sync + 'static,
    {
        Self {
            entries: Mutex::new(HashMap::new()),
            hash_fn: Arc::new(hash_fn),
        }
    }

    /// Acquires the lock selected by `id`.
    ///
    /// The returned guard releases the lock when dropped. Waiting is
    /// cancellation-safe and uses the caller's Tokio runtime.
    pub async fn lock(&self, id: u32) -> OwnedMutexGuard<()> {
        let hashed_id = (self.hash_fn)(id);
        let entry = {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(
                entries
                    .entry(hashed_id)
                    .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
            )
        };

        entry.lock_owned().await
    }
}

impl Default for LockGroup {
    fn default() -> Self {
        Self::new()
    }
}
