// Copyright 2026 AsterSQL.
//! Sampled store loads, isolated from rolling heartbeat windows.
use astersql_pd_core::StoreInfo;
use astersql_pd_core_constant::ResourceKind;
use astersql_pd_statistics_utils::{DIM_LEN, RW_TYPE_LEN, RWType};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

/// Current byte, key, query and CPU load rates.
pub type Loads = [f64; DIM_LEN];
/// Independent snapshots in dimension/physical-ring-slot order.
pub type HistoryLoads = [Vec<f64>; DIM_LEN];
pub const DEFAULT_HISTORY_SAMPLE_INTERVAL: Duration = Duration::from_secs(30);
pub const DEFAULT_HISTORY_SAMPLE_DURATION: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
struct StoreHistoryLoad {
    history: HistoryLoads,
    next: usize,
    update: Option<Duration>,
}

/// Caller-owned history; use an external lock if shared across threads.
#[derive(Clone, Debug)]
pub struct StoreHistoryLoads {
    loads: [[HashMap<u64, StoreHistoryLoad>; 3]; RW_TYPE_LEN],
    sample_duration: Duration,
    sample_interval: Duration,
}
impl StoreHistoryLoads {
    pub fn new(sample_duration: Duration, sample_interval: Duration) -> Self {
        Self {
            loads: std::array::from_fn(|_| std::array::from_fn(|_| HashMap::new())),
            sample_duration,
            sample_interval,
        }
    }
    /// Add using a host-supplied monotonic timestamp. The first point is immediate.
    /// Like Go Add(ID), this does not validate store liveness; prefer add_store
    /// when current metadata is available. Unknown RW/resource indexes panic.
    pub fn add(
        &mut self,
        store_id: u64,
        rw: RWType,
        kind: ResourceKind,
        point: Loads,
        now: Duration,
    ) {
        let size = if self.sample_duration.is_zero() {
            0
        } else if self.sample_interval.is_zero() {
            (DEFAULT_HISTORY_SAMPLE_DURATION.as_nanos()
                / DEFAULT_HISTORY_SAMPLE_INTERVAL.as_nanos()) as usize
        } else {
            usize::try_from(self.sample_duration.as_nanos() / self.sample_interval.as_nanos())
                .expect("history capacity exceeds address space")
        };
        let load = self.loads[rw.as_i32() as usize][kind.as_i32() as usize]
            .entry(store_id)
            .or_insert_with(|| StoreHistoryLoad {
                history: HistoryLoads::default(),
                next: 0,
                update: None,
            });
        if size == 0
            || load.update.is_some_and(|last| {
                now.checked_sub(last)
                    .is_none_or(|elapsed| elapsed < self.sample_interval)
            })
        {
            return;
        }
        if load.update.is_none() {
            load.history = std::array::from_fn(|_| vec![0.0; size]);
        }
        for (dimension, value) in point.into_iter().enumerate() {
            load.history[dimension][load.next] = value;
        }
        load.next = (load.next + 1) % size;
        load.update = Some(now);
    }
    /// Sampling entry point for callers holding current store metadata.
    pub fn add_store(
        &mut self,
        store: &StoreInfo,
        rw: RWType,
        kind: ResourceKind,
        point: Loads,
        now: Duration,
    ) {
        if !store.is_removed() {
            self.add(store.id(), rw, kind, point, now);
        }
    }
    /// Return a deep snapshot, or empty vectors for a missing/unsampled entry.
    pub fn get(&self, store_id: u64, rw: RWType, kind: ResourceKind) -> HistoryLoads {
        self.loads[rw.as_i32() as usize][kind.as_i32() as usize]
            .get(&store_id)
            .map(|h| h.history.clone())
            .unwrap_or_default()
    }
    /// Retain only IDs present with a non-Removed node state, across all kinds.
    pub fn gc<'a>(&mut self, stores: impl IntoIterator<Item = &'a StoreInfo>) {
        let alive: HashSet<u64> = stores
            .into_iter()
            .filter(|s| !s.is_removed())
            .map(StoreInfo::id)
            .collect();
        for by_kind in &mut self.loads {
            for by_store in by_kind {
                by_store.retain(|id, _| alive.contains(id));
            }
        }
    }
    /// Return whether configuration changed and history was reset.
    pub fn update_config(&mut self, duration: Duration, interval: Duration) -> bool {
        if (duration, interval) == (self.sample_duration, self.sample_interval) {
            return false;
        }
        *self = Self::new(duration, interval);
        true
    }
}
impl Default for StoreHistoryLoads {
    fn default() -> Self {
        Self::new(
            DEFAULT_HISTORY_SAMPLE_DURATION,
            DEFAULT_HISTORY_SAMPLE_INTERVAL,
        )
    }
}
