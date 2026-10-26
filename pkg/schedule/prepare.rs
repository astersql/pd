// Copyright 2026 AsterSQL.
//! Scheduling admission without a second heartbeat collection registry.
use astersql_pd_core::BasicCluster;
use std::{sync::Mutex, time::Duration};

/// Maximum wait before scheduling admission, matching Go's five minutes.
pub const COLLECT_TIMEOUT: Duration = Duration::from_secs(300);

/// Sticky preparation gate. The host supplies monotonic time and the current
/// total-region provider; all region observations stay in the basic cluster.
/// The provider must not re-enter this gate while its lock is held.
pub struct PrepareChecker<F> {
    start: Duration,
    prepared: Mutex<bool>,
    total_region_count: F,
}

impl<F, E> PrepareChecker<F>
where
    F: Fn() -> Result<i64, E>,
{
    /// Start an unprepared gate at a host monotonic timestamp.
    pub fn new(start: Duration, total_region_count: F) -> Self {
        Self {
            start,
            prepared: Mutex::new(false),
            total_region_count,
        }
    }

    /// Check admission. Provider errors deny this attempt without latching
    /// readiness, as in Go; timeout and an already prepared gate bypass I/O.
    pub fn check(&self, cluster: &BasicCluster, now: Duration) -> bool {
        let mut prepared = self.prepared.lock().unwrap();
        if *prepared {
            return true;
        }
        if now.saturating_sub(self.start) > COLLECT_TIMEOUT {
            *prepared = true;
            return true;
        }
        let Ok(total) = (self.total_region_count)() else {
            return false;
        };
        if total as f64 * 0.9 > cluster.regions.not_from_storage_regions_count() as f64 {
            return false;
        }
        for store in cluster.stores.get_stores() {
            if !store.is_preparing() && !store.is_serving() {
                continue;
            }
            if store.store_stats().region_count as f64 * 0.9
                > cluster
                    .regions
                    .not_from_storage_regions_count_by_store(store.id()) as f64
                || !cluster.regions.is_store_prepared(store.id())
            {
                return false;
            }
        }
        *prepared = true;
        true
    }

    /// Coordinator admission entry point, delegating to this same gate.
    pub fn should_run(&self, cluster: &BasicCluster, now: Duration) -> bool {
        self.check(cluster, now)
    }

    /// Read readiness without checking or collecting regions.
    pub fn is_prepared(&self) -> bool {
        *self.prepared.lock().unwrap()
    }

    /// Explicit test override, matching Go's SetPrepared.
    pub fn set_prepared(&self) {
        *self.prepared.lock().unwrap() = true;
    }
}
