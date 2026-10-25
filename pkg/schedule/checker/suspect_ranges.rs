// Copyright 2026 AsterSQL.
//! Suspect key ranges feed the same pending cache consumed by patrol.
use astersql_pd_cache::TTLString;
use astersql_pd_core::BasicCluster;
use astersql_pd_keyutil::build_key_range_key;
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

pub const SUSPECT_REGION_LIMIT: isize = 1024;
pub const SUSPECT_RANGE_INTERVAL: Duration = Duration::from_millis(100);
/// Exact range identity, not an interval union. The host owns TTL cache shutdown.
pub struct SuspectRanges {
    ranges: Arc<TTLString<[Vec<u8>; 2]>>,
}
impl SuspectRanges {
    pub fn new(ranges: Arc<TTLString<[Vec<u8>; 2]>>) -> Self {
        Self { ranges }
    }
    pub fn add(&self, start: &[u8], end: &[u8]) {
        self.ranges.put(
            build_key_range_key(start, end),
            [start.to_vec(), end.to_vec()],
        );
    }
    pub fn pop(&self) -> Option<[Vec<u8>; 2]> {
        self.ranges.pop().map(|(_, range)| range)
    }
    pub fn clear(&self) {
        self.ranges.clear();
    }
    /// One tick processes at most one arbitrary live range. Empty scans consume it.
    pub fn check_once(
        &self,
        cluster: &BasicCluster,
        dispatcher: &crate::CheckerDispatcher,
    ) -> Vec<u64> {
        let Some([start, end]) = self.pop() else {
            return Vec::new();
        };
        let regions = cluster
            .regions
            .scan_regions(&start, &end, SUSPECT_REGION_LIMIT);
        let Some(last) = regions.last() else {
            return Vec::new();
        };
        let last_end = &last.meta().key_range.end_key;
        // Preserve literal Go bytes.Compare: an empty requested end is not infinity here.
        if !last_end.is_empty() && last_end < &end {
            self.add(last_end, &end);
        }
        let ids: Vec<_> = regions.iter().map(|r| r.meta().id).collect();
        dispatcher.add_pending_processed_regions(false, &ids);
        ids
    }
    /// Runs on the host executor. Each callback acquires the host's coherent
    /// cluster/dispatcher snapshot and calls `check_once`. This permits heartbeat
    /// updates between ticks without holding a registry borrow for the worker lifetime.
    /// Cancel and await (including an in-flight callback) before cache shutdown.
    pub async fn run<F, Fut, E>(&self, mut check: F, cancel: CancellationToken) -> Result<(), E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<(), E>>,
    {
        let mut ticker = tokio::time::interval_at(
            Instant::now() + SUSPECT_RANGE_INTERVAL,
            SUSPECT_RANGE_INTERVAL,
        );
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                _ = ticker.tick() => { check().await?; }
            }
        }
    }
}
