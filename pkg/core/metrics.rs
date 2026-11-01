// Copyright 2026 AsterSQL.
//! Host-owned core collectors. Handles are cached for fixed domain labels.
use prometheus::{
    Counter, CounterVec, Histogram, HistogramOpts, HistogramVec, Opts, core::Collector,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const HEARTBEAT_STAGES: [&str; 9] = [
    "PreCheck",
    "AsyncHotStatsDuration",
    "RegionGuide",
    "SaveCache_CheckOverlaps",
    "SaveCache_InvalidRegion",
    "SaveCache_SetRegion",
    "SaveCache_UpdateSubTree",
    "CollectRegionStats",
    "Other",
];

/// Registrations belong to the host; cloning a domain handle never registers.
pub struct CoreMetrics {
    collectors: Vec<Arc<dyn Collector>>,
    scheduler: std::sync::OnceLock<Arc<dyn SchedulerMetricSink>>,
    wait_sum: [Counter; 2],
    wait_count: [Counter; 2],
    stage_sum: [Counter; 9],
    stage_count: [Counter; 9],
    query_time: [Histogram; 3],
    query_count: [Counter; 3],
    bucket_events: [Counter; 5],
    region_events: std::sync::OnceLock<[Counter; 2]>,
    breakdown: AtomicBool,
    waits: [AtomicU64; 4],
    last_waits: std::sync::Mutex<[u64; 4]>,
}
impl Default for CoreMetrics {
    fn default() -> Self {
        Self::new().expect("static core metric definitions")
    }
}
impl CoreMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let stages = CounterVec::new(
            Opts::new(
                "pd_core_region_heartbeat_breakdown_handle_duration_seconds_sum",
                "Bucketed histogram of processing time (s) of handle the heartbeat stage.",
            ),
            &["name"],
        )?;
        let stage_count = CounterVec::new(
            Opts::new(
                "pd_core_region_heartbeat_breakdown_handle_duration_seconds_count",
                "Bucketed histogram of processing count of handle the heartbeat stage.",
            ),
            &["name"],
        )?;
        let wait = CounterVec::new(
            Opts::new(
                "pd_core_acquire_regions_lock_wait_duration_seconds_sum",
                "Bucketed histogram of processing time (s) of waiting for acquiring regions lock.",
            ),
            &["type"],
        )?;
        // Preserve Go's distinct label names: duration uses type, count uses name.
        let waits = CounterVec::new(
            Opts::new(
                "pd_core_acquire_regions_lock_wait_duration_seconds_count",
                "Bucketed histogram of processing count of waiting for acquiring regions lock.",
            ),
            &["name"],
        )?;
        let query_time = HistogramVec::new(
            HistogramOpts::new(
                "pd_core_query_region_duration_seconds",
                "Bucketed histogram of processing time (s) of region query.",
            )
            .buckets(prometheus::exponential_buckets(0.0005, 2.0, 13)?),
            &["type"],
        )?;
        let query_count = CounterVec::new(
            Opts::new(
                "pd_core_query_region_count",
                "The number of region query types.",
            ),
            &["type"],
        )?;
        let bucket_events = CounterVec::new(
            Opts::new("pd_cluster_bucket_event", "Counter of the bucket event"),
            &["event"],
        )?;
        Ok(Self {
            bucket_events: [
                "version_stale",
                "version_no_change",
                "region_cache_miss",
                "update_failed",
                "update_success",
            ]
            .map(|l| bucket_events.with_label_values(&[l])),
            wait_sum: ["WaitRegionsLock", "WaitSubRegionsLock"]
                .map(|l| wait.with_label_values(&[l])),
            wait_count: ["WaitRegionsLock", "WaitSubRegionsLock"]
                .map(|l| waits.with_label_values(&[l])),
            stage_sum: HEARTBEAT_STAGES.map(|l| stages.with_label_values(&[l])),
            stage_count: HEARTBEAT_STAGES.map(|l| stage_count.with_label_values(&[l])),
            query_time: ["by-keys", "by-prev-keys", "by-ids"]
                .map(|l| query_time.with_label_values(&[l])),
            query_count: ["keys", "prev-keys", "ids"].map(|l| query_count.with_label_values(&[l])),
            collectors: vec![
                Arc::new(stages),
                Arc::new(stage_count),
                Arc::new(wait),
                Arc::new(waits),
                Arc::new(query_time),
                Arc::new(query_count),
                Arc::new(bucket_events),
            ],
            region_events: Default::default(),
            scheduler: Default::default(),
            breakdown: AtomicBool::new(false),
            waits: Default::default(),
            last_waits: Default::default(),
        })
    }
    /// Attach host-owned scheduling collectors without retaining a registry.
    pub fn set_scheduler_metrics(&self, sink: Arc<dyn SchedulerMetricSink>) {
        let _ = self.scheduler.set(sink);
    }
    /// Cache a bounded event handle before a policy enters its candidate loops.
    pub fn scheduler_event_metrics(&self, scheduler: &str, event: &str) -> Option<Counter> {
        self.scheduler_counter_metrics("pd_scheduler_event_count", &[scheduler, event])
    }
    pub fn scheduler_counter_metrics(&self, family: &str, labels: &[&str]) -> Option<Counter> {
        self.scheduler
            .get()
            .map(|sink| sink.counter_metrics(family, labels))
    }
    pub fn collectors(&self) -> impl Iterator<Item = Box<dyn Collector>> + '_ {
        self.collectors
            .iter()
            .map(|c| Box::new(CoreCollector(c.clone())) as Box<dyn Collector>)
    }
    pub fn set_breakdown_metrics(&self, enabled: bool) {
        self.breakdown.store(enabled, Ordering::Relaxed);
    }
    pub fn stage_metrics(&self, stage: usize) -> StageMetrics<'_> {
        StageMetrics {
            metrics: self,
            stage,
            begin: self.breakdown.load(Ordering::Relaxed).then(Instant::now),
        }
    }
    pub fn set_region_metrics(&self, events: [Counter; 2]) {
        let _ = self.region_events.set(events);
    }
    pub fn region_event_metrics(&self, event: usize) {
        if let Some(events) = self.region_events.get() {
            events[event].inc();
        }
    }
    pub fn bucket_event_metrics(&self, event: usize) {
        self.bucket_events[event].inc();
    }
    pub fn observe_wait_metrics(&self, subtree: bool, elapsed: Duration) {
        let i = usize::from(subtree) * 2;
        self.waits[i].fetch_add(
            elapsed.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        self.waits[i + 1].fetch_add(1, Ordering::Relaxed);
    }
    /// Go's 15-second sampler drops the first baseline and invalid long windows.
    pub fn collect_wait_metrics(&self) {
        let current = std::array::from_fn::<_, 4, _>(|i| self.waits[i].load(Ordering::Relaxed));
        let mut previous = self.last_waits.lock().unwrap();
        let old = std::mem::replace(&mut *previous, current);
        if old.contains(&0)
            || (0..4).any(|i| current[i] < old[i])
            || [0, 2]
                .into_iter()
                .any(|i| current[i] - old[i] > 15_000_000_000)
        {
            return;
        }
        for i in 0..2 {
            self.wait_sum[i].inc_by((current[i * 2] - old[i * 2]) as f64 / 1e9);
            self.wait_count[i].inc_by((current[i * 2 + 1] - old[i * 2 + 1]) as f64);
        }
    }
    pub fn observe_query_metrics(&self, kind: usize, count: usize, elapsed: Duration) {
        if count > 0 {
            self.query_count[kind].inc_by(count as f64);
            self.query_time[kind].observe(elapsed.as_secs_f64());
        }
    }
}
/// Measures only the actual stage scope, including a stage ending in error.
pub struct StageMetrics<'a> {
    metrics: &'a CoreMetrics,
    stage: usize,
    begin: Option<Instant>,
}
impl Drop for StageMetrics<'_> {
    fn drop(&mut self) {
        if let Some(begin) = self.begin {
            self.metrics.stage_sum[self.stage].inc_by(begin.elapsed().as_secs_f64());
            self.metrics.stage_count[self.stage].inc();
        }
    }
}

struct CoreCollector(Arc<dyn Collector>);
impl Collector for CoreCollector {
    fn desc(&self) -> Vec<&prometheus::core::Desc> {
        self.0.desc()
    }
    fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.0.collect()
    }
}

/// Optional scheduling observation. Stores attach it only through live membership;
/// snapshots retain no registry or transport dependency.
pub trait StoreMetricSink: Send + Sync {
    fn rejected_metrics(&self, scope: &str, kind: &str, source: bool, id: u64);
    fn delete_store_metrics(&self, id: u64);
    fn balance_metrics(
        &self,
        _scheduler: &str,
        _source: bool,
        _id: u64,
        _influence: i64,
        _tolerance: i64,
    ) {
    }
    fn scatter_metrics(&self, _id: u64, _leader: bool, _engine: &str) {}
}
pub(crate) struct LiveStoreMetrics {
    pub stores: std::sync::Weak<
        std::sync::RwLock<std::collections::HashMap<u64, std::sync::Arc<crate::StoreInfo>>>,
    >,
    pub sink: std::sync::Arc<dyn StoreMetricSink>,
}
impl StoreMetricSink for LiveStoreMetrics {
    fn balance_metrics(
        &self,
        scheduler: &str,
        source: bool,
        id: u64,
        influence: i64,
        tolerance: i64,
    ) {
        let Some(stores) = self.stores.upgrade() else {
            return;
        };
        let stores = stores.read().unwrap();
        if stores.get(&id).is_some_and(|store| !store.is_removed()) {
            self.sink
                .balance_metrics(scheduler, source, id, influence, tolerance);
        }
    }
    fn rejected_metrics(&self, scope: &str, kind: &str, source: bool, id: u64) {
        let Some(stores) = self.stores.upgrade() else {
            return;
        };
        let stores = stores.read().unwrap();
        // Keep membership locked through publication. Deletion takes the write
        // lock before cleanup, so an older snapshot cannot recreate a series.
        if stores.get(&id).is_some_and(|store| !store.is_removed()) {
            self.sink.rejected_metrics(scope, kind, source, id);
        }
    }
    fn delete_store_metrics(&self, id: u64) {
        self.sink.delete_store_metrics(id);
    }
    fn scatter_metrics(&self, id: u64, leader: bool, engine: &str) {
        let Some(stores) = self.stores.upgrade() else {
            return;
        };
        let stores = stores.read().unwrap();
        if stores.get(&id).is_some_and(|s| !s.is_removed()) {
            self.sink.scatter_metrics(id, leader, engine);
        }
    }
}

/// Optional observation bridge; scheduler libraries supply actual event data.
/// Handles contain values only, never a registry, transport or worker.
pub trait SchedulerMetricSink: Send + Sync {
    fn counter_metrics(&self, family: &str, labels: &[&str]) -> Counter;
}
