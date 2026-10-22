// Copyright 2026 AsterSQL.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::blocking::Mutex;

type HashFn = dyn Fn(u32) -> u32 + Send + Sync;

struct LockEntry {
    mutex: Arc<AsyncMutex<()>>,
    references: usize,
}

/// Serializes work sharing the same hashed ID while leaving other IDs independent.
pub struct LockGroup {
    entries: Mutex<HashMap<u32, LockEntry>>,
    hash_fn: Arc<HashFn>,
    remove_entry_on_unlock: bool,
}

/// Releases a lock-group entry when dropped.
pub struct LockGroupGuard<'a> {
    guard: Option<OwnedMutexGuard<()>>,
    reservation: Option<LockReservation<'a>>,
}

struct LockReservation<'a> {
    group: &'a LockGroup,
    hashed_id: u32,
    mutex: Arc<AsyncMutex<()>>,
    active: bool,
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
            remove_entry_on_unlock: false,
        }
    }

    /// Creates a lock group that can remove an ID's entry after its last user
    /// releases or cancels its reservation.
    pub fn with_remove_entry_on_unlock(remove_entry_on_unlock: bool) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            hash_fn: Arc::new(|id| id),
            remove_entry_on_unlock,
        }
    }

    /// Acquires the lock selected by `id`.
    ///
    /// The returned guard releases the lock when dropped. Waiting is
    /// cancellation-safe and uses the caller's Tokio runtime.
    pub async fn lock(&self, id: u32) -> LockGroupGuard<'_> {
        let hashed_id = (self.hash_fn)(id);
        let mutex = {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let entry = entries.entry(hashed_id).or_insert_with(|| LockEntry {
                mutex: Arc::new(AsyncMutex::new(())),
                references: 0,
            });
            entry.references += 1;
            Arc::clone(&entry.mutex)
        };
        let reservation = LockReservation {
            group: self,
            hashed_id,
            mutex: Arc::clone(&mutex),
            active: true,
        };
        let guard = mutex.lock_owned().await;

        LockGroupGuard {
            guard: Some(guard),
            reservation: Some(reservation),
        }
    }

    fn release(
        &self,
        hashed_id: u32,
        mutex: &Arc<AsyncMutex<()>>,
        guard: Option<OwnedMutexGuard<()>>,
    ) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let should_remove = entries.get_mut(&hashed_id).is_some_and(|entry| {
            if !Arc::ptr_eq(&entry.mutex, mutex) {
                return false;
            }
            entry.references -= 1;
            self.remove_entry_on_unlock && entry.references == 0
        });
        if should_remove {
            entries.remove(&hashed_id);
        }
        // Keep the entries map locked until the old mutex is unlocked. Once
        // an entry is removed, a new caller must not create and acquire its
        // replacement while the old guard is still held.
        drop(guard);
    }

    #[cfg(test)]
    pub(crate) fn entry_count(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    #[cfg(test)]
    pub(crate) fn reference_count(&self, id: u32) -> usize {
        let hashed_id = (self.hash_fn)(id);
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&hashed_id)
            .map_or(0, |entry| entry.references)
    }
}

impl Drop for LockGroupGuard<'_> {
    fn drop(&mut self) {
        if let Some(mut reservation) = self.reservation.take() {
            reservation.active = false;
            reservation
                .group
                .release(reservation.hashed_id, &reservation.mutex, self.guard.take());
        }
    }
}

impl Drop for LockReservation<'_> {
    fn drop(&mut self) {
        if self.active {
            self.group.release(self.hashed_id, &self.mutex, None);
        }
    }
}

impl Default for LockGroup {
    fn default() -> Self {
        Self::new()
    }
}
