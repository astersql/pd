// Copyright 2026 AsterSQL.
//! Batched filter counts and host-owned Prometheus series lifecycle.
use crate::FilterType;
use astersql_pd_schedule_core::BasicCluster;
use prometheus::{Counter, CounterVec, Opts, Registry};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

const FILTER_NAMES: [&str; 26] = [
    "exclude-filter",
    "storage-threshold-filter",
    "distinct-filter",
    "label-constraint-filter",
    "rule-fit-filter",
    "rule-fit-leader-filter",
    "engine-filter",
    "special-use-filter",
    "isolation-filter",
    "leader-evicted-filter",
    "store-state-ok-filter",
    "store-state-tombstone-filter",
    "store-state-down-filter",
    "store-state-offline-filter",
    "store-state-pause-leader-filter",
    "store-state-slow-filter",
    "store-state-stopping-filter",
    "store-state-disconnect-filter",
    "store-state-busy-filter",
    "store-state-exceed-remove-limit-filter",
    "store-state-exceed-add-limit-filter",
    "store-state-exceed-transfer-leader-in-limit-filter",
    "store-state-too-many-snapshots-filter",
    "store-state-too-many-pending-peers-filter",
    "store-state-reject-leader-filter",
    "store-state-slow-trend-filter",
];
impl FilterType {
    /// Stable Go filter label; identities >= 26 return `unknown`.
    /// Negative identities panic, matching Go's array indexing.
    pub fn as_str(self) -> &'static str {
        if self.0 < FILTER_NAMES.len() as i64 {
            FILTER_NAMES[usize::try_from(self.0).expect("negative Go filter identity")]
        } else {
            "unknown"
        }
    }
}

/// Direction of the rejected store, independent of filter identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterAction {
    Source,
    Target,
}
impl FilterAction {
    /// Go action's diagnostic name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Source => "filter-source",
            Self::Target => "filter-target",
        }
    }
}

type SeriesKey = (FilterAction, String, FilterType, u64);
struct MetricsInner {
    source: CounterVec,
    target: CounterVec,
    // Cache handles only while their series exists. One lock serializes flush,
    // deletion and reset, so an old cached handle cannot detach later updates.
    series: Mutex<HashMap<SeriesKey, Counter>>,
}
/// Two compatible metric vectors belonging to one host Registry.
/// Clones share that host; independent registrations share no process state.
#[derive(Clone)]
pub struct FilterMetrics {
    inner: Arc<MetricsInner>,
}
impl FilterMetrics {
    /// Register both vectors, rolling back only our source if the target fails.
    pub fn register(registry: &Registry) -> prometheus::Result<Self> {
        let source = CounterVec::new(
            Opts::new("pd_schedule_source_filter", "Counter of the source filter"),
            &["scope", "type", "source"],
        )?;
        let target = CounterVec::new(
            Opts::new("pd_schedule_target_filter", "Counter of the target filter"),
            &["scope", "type", "target"],
        )?;
        registry.register(Box::new(source.clone()))?;
        if let Err(error) = registry.register(Box::new(target.clone())) {
            registry.unregister(Box::new(source))?;
            return Err(error);
        }
        Ok(Self {
            inner: Arc::new(MetricsInner {
                source,
                target,
                series: Mutex::new(HashMap::new()),
            }),
        })
    }
    /// Delete every scope and filter for exactly this decimal store label.
    pub fn delete_store_metrics(&self, store_id: &str) {
        let mut series = self.inner.series.lock().unwrap();
        series.retain(|(action, scope, kind, id), _| {
            if id.to_string() != store_id {
                return true;
            }
            let vector = match action {
                FilterAction::Source => &self.inner.source,
                FilterAction::Target => &self.inner.target,
            };
            // Entries only exist after successful creation in these private vectors.
            vector
                .remove_label_values(&[scope.as_str(), kind.as_str(), store_id])
                .expect("registered filter series");
            false
        });
    }
    /// Clear this host's published counts and invalidate all cached handles.
    /// Pending batches are separate and follow Go's flush semantics.
    pub fn reset_filter_metrics(&self) {
        let mut series = self.inner.series.lock().unwrap();
        self.inner.source.reset();
        self.inner.target.reset();
        series.clear();
    }
}

/// Per-owner pending counts; increments do no Prometheus lookups.
/// Exclusive mutation matches Go's scheduler-owned, non-concurrent Counter.
#[derive(Default)]
pub struct FilterCounter {
    scope: String,
    pending: HashMap<(FilterAction, FilterType, u64), u64>,
}
impl FilterCounter {
    /// Start an empty batch under the scheduler/checker scope.
    pub fn new(scope: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            ..Default::default()
        }
    }
    /// Change the scope used by the next flush, including already pending counts.
    pub fn set_scope(&mut self, scope: impl Into<String>) {
        self.scope = scope.into();
    }
    /// Count a rejection without creating a metric series.
    /// Panics for invalid filter identities, matching Go's indexed counter.
    pub fn inc(&mut self, action: FilterAction, kind: FilterType, store_id: u64) {
        assert!((0..26).contains(&kind.0), "invalid filter identity");
        *self.pending.entry((action, kind, store_id)).or_default() += 1;
    }
    /// Read an unflushed count; consumed or absent entries are zero.
    pub fn pending(&self, action: FilterAction, kind: FilterType, store_id: u64) -> u64 {
        self.pending
            .get(&(action, kind, store_id))
            .copied()
            .unwrap_or_default()
    }
    /// Publish and consume this batch. Re-read the canonical host store state,
    /// discarding missing/Removed stores before creating or accessing a series.
    /// `None` retains the Go nil-informer behavior. The host must publish removal
    /// before deleting metrics; a later flush then cannot resurrect that store.
    pub fn flush(&mut self, metrics: &FilterMetrics, cluster: Option<&dyn BasicCluster>) {
        let mut series = metrics.inner.series.lock().unwrap();
        for ((action, kind, id), value) in self.pending.drain() {
            if value == 0 {
                continue;
            }
            if cluster.is_some_and(|c| c.get_store(id).is_none_or(|s| s.is_removed())) {
                continue;
            }
            let vector = match action {
                FilterAction::Source => &metrics.inner.source,
                FilterAction::Target => &metrics.inner.target,
            };
            let metric = series
                .entry((action, self.scope.clone(), kind, id))
                .or_insert_with(|| {
                    vector.with_label_values(&[self.scope.as_str(), kind.as_str(), &id.to_string()])
                });
            metric.inc_by(value as f64);
        }
    }
}
