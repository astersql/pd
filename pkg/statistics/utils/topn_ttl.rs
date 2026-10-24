// Copyright 2026 AsterSQL.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use crate::{TopN, TopNItem};

/// A multi-dimensional TopN with explicit, host-clock-driven expiration.
///
/// Timestamps are elapsed durations from one host-selected monotonic origin.
/// Reads and insertion do not implicitly expire items. Call `remove_expired` to
/// clean up; renewing an expired but retained ID still counts as an update.
/// This type owns no timer, thread, runtime or global clock.
pub struct TtlTopN<T> {
    inner: TopN<T>,
    dimensions: usize,
    ttl: Duration,
    deadlines: BTreeSet<(Duration, u64)>,
    by_id: HashMap<u64, Duration>,
}

impl<T: TopNItem> TtlTopN<T> {
    /// Creates a collection; zero TTL expires at the insertion timestamp.
    /// Panics for zero dimensions or capacity, like `TopN::new`.
    pub fn new(dimensions: usize, n: usize, ttl: Duration) -> Self {
        Self {
            inner: TopN::new(dimensions, n),
            dimensions,
            ttl,
            deadlines: BTreeSet::new(),
            by_id: HashMap::new(),
        }
    }

    /// Inserts/replaces an item and renews its deadline; true means replaced.
    /// Panics before mutation if the timestamp plus TTL overflows Duration.
    pub fn put(&mut self, item: T, now: Duration) -> bool {
        let deadline = now.checked_add(self.ttl).expect("TTL deadline overflow");
        let id = item.id();
        let updated = self.inner.put(item);
        if let Some(old) = self.by_id.insert(id, deadline) {
            self.deadlines.remove(&(old, id));
        }
        self.deadlines.insert((deadline, id));
        updated
    }

    /// Removes every item whose deadline is at or before `now`, in every dimension.
    /// Each expired ID is returned once per dimension, matching Go RemoveExpired.
    /// IDs with equal deadlines have no promised relative order.
    pub fn remove_expired(&mut self, now: Duration) -> Vec<u64> {
        let mut removed = Vec::new();
        while let Some(&(deadline, id)) = self.deadlines.first() {
            if deadline > now {
                break;
            }
            self.deadlines.pop_first();
            self.by_id.remove(&id);
            self.inner.remove(id);
            removed.extend(std::iter::repeat_n(id, self.dimensions));
        }
        removed
    }

    /// Removes an identity and its deadline, returning the latest item.
    pub fn remove(&mut self, id: u64) -> Option<Arc<T>> {
        if let Some(deadline) = self.by_id.remove(&id) {
            self.deadlines.remove(&(deadline, id));
        }
        self.inner.remove(id)
    }

    /// Read-only access to the full collection and per-dimension top sets.
    /// Mutable access is withheld so every mutation also maintains deadlines.
    pub fn items(&self) -> &TopN<T> {
        &self.inner
    }
}
