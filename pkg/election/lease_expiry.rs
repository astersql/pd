// Copyright 2026 AsterSQL.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Monotonic time source. Implementations must never move backwards.
pub trait LeaseClock: Send + Sync {
    fn now(&self) -> Instant;
}
/// Host monotonic clock; wall clock adjustments cannot extend a lease.
pub struct MonotonicLeaseClock;
impl LeaseClock for MonotonicLeaseClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Local conservative lease deadline, independent of backend ownership checks.
pub(crate) struct LeaseExpiry {
    clock: Arc<dyn LeaseClock>,
    deadline: Mutex<Option<Instant>>,
}
impl LeaseExpiry {
    /// A lease without a successful grant is expired.
    pub(crate) fn new(clock: Arc<dyn LeaseClock>) -> Self {
        Self {
            clock,
            deadline: Mutex::new(None),
        }
    }
    pub(crate) fn request_start(&self) -> Instant {
        self.clock.now()
    }
    pub(crate) fn granted(&self, start: Instant, ttl_seconds: i64) {
        *self.deadline.lock().unwrap() = Self::deadline(start, ttl_seconds);
    }
    pub(crate) fn renewed(&self, start: Instant, ttl_seconds: i64) {
        let Some(candidate) = Self::deadline(start, ttl_seconds) else {
            return;
        };
        let mut deadline = self.deadline.lock().unwrap();
        // None is ungranted/closed: a late renewal must not resurrect it.
        if let Some(current) = deadline.as_mut() {
            *current = (*current).max(candidate);
        }
    }
    fn deadline(start: Instant, ttl_seconds: i64) -> Option<Instant> {
        if ttl_seconds <= 0 {
            return None;
        }
        start.checked_add(Duration::from_secs(ttl_seconds as u64))
    }
    pub(crate) fn close(&self) {
        *self.deadline.lock().unwrap() = None;
    }
    /// As in Go IsExpired, equality with the deadline is still valid.
    pub(crate) fn is_expired(&self) -> bool {
        self.deadline
            .lock()
            .unwrap()
            .is_none_or(|deadline| self.clock.now() > deadline)
    }
}
