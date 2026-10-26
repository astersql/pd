// Copyright 2026 AsterSQL.
//! Host-owned scheduler collectors and explicit lifecycle boundaries.
use prometheus::{
    Counter, CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry, core::Collector,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

struct Definition {
    name: &'static str,
    help: &'static str,
    labels: &'static [&'static str],
    kind: &'static str,
    reset: bool,
    delete_store: bool,
}
const DEFINITIONS: &[Definition] = &[
    Definition {
        name: "pd_scheduler_status",
        help: "Status of the scheduler.",
        labels: &["kind", "type"],
        kind: "gauge",
        reset: true,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_event_count",
        help: "Counter of scheduler events.",
        labels: &["type", "name"],
        kind: "counter",
        reset: false,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_op_influence",
        help: "Store status for schedule",
        labels: &["scheduler", "store", "type"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_tolerant_resource",
        help: "Store status for schedule",
        labels: &["scheduler"],
        kind: "gauge",
        reset: false,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_balance_witness",
        help: "Counter of balance witness scheduler.",
        labels: &["type", "store"],
        kind: "counter",
        reset: true,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_hot_region",
        help: "Counter of hot region scheduler.",
        labels: &["type", "store"],
        kind: "counter",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_balance_direction",
        help: "Counter of direction of balance related schedulers.",
        labels: &["type", "store", "direction"],
        kind: "counter",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_hot_region_direction",
        help: "Counter of hot region scheduler.",
        labels: &["type", "rw", "store", "direction", "dim"],
        kind: "counter",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_hot_peer",
        help: "Bucketed histogram of the scheduling hot peer.",
        labels: &["type", "rw", "dim"],
        kind: "histogram",
        reset: false,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_evicted_slow_store_status",
        help: "Store evicted status due to slow",
        labels: &["store", "slow_type"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_evicted_stopping_store_status",
        help: "Store evicted status due to stopping",
        labels: &["store"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_slow_store_trigger_limit",
        help: "slow store trigger limit",
        labels: &["store", "slow_type"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_store_slow_trend_evicted_status",
        help: "Store evicted by slow trend status for schedule",
        labels: &["address", "store"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_scheduler_store_slow_trend_action_status",
        help: "Store trend scheduler calculating actions",
        labels: &["type", "status"],
        kind: "gauge",
        reset: false,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_store_slow_trend_misc",
        help: "Store trend internal uncatalogued values",
        labels: &["type", "dim"],
        kind: "gauge",
        reset: false,
        delete_store: false,
    },
    Definition {
        name: "pd_scheduler_hot_pending_sum",
        help: "Pending influence sum of store in hot region scheduler.",
        labels: &["store", "rw", "dim"],
        kind: "gauge",
        reset: false,
        delete_store: true,
    },
    Definition {
        name: "pd_rule_manager_status",
        help: "Status of the rule.",
        labels: &["type"],
        kind: "gauge",
        reset: true,
        delete_store: false,
    },
    Definition {
        name: "pd_region_label_status",
        help: "Status of the region labeler.",
        labels: &["type"],
        kind: "gauge",
        reset: true,
        delete_store: false,
    },
    Definition {
        name: "pd_balance_range_store",
        help: "Store status for balance range schedule",
        labels: &["store", "type"],
        kind: "gauge",
        reset: true,
        delete_store: true,
    },
    Definition {
        name: "pd_balance_range_job",
        help: "job status for balance range schedule",
        labels: &[],
        kind: "gauge",
        reset: false,
        delete_store: false,
    },
];
// Frozen eager event vocabulary from Go package initialization.
const INITIAL_EVENTS: &[(&str, &str)] = &[
    ("balance-hot-region-scheduler", "abnormal_replica"),
    ("balance-hot-region-scheduler", "create_operator_failed"),
    ("balance-hot-region-scheduler", "hot_buckets_not_valid"),
    ("balance-hot-region-scheduler", "ignored_affinity"),
    (
        "balance-hot-region-scheduler",
        "need_split_before_move_peer",
    ),
    ("balance-hot-region-scheduler", "new_operator"),
    ("balance-hot-region-scheduler", "no_region"),
    ("balance-hot-region-scheduler", "not_found_split_keys"),
    ("balance-hot-region-scheduler", "not_same_engine"),
    ("balance-hot-region-scheduler", "only_one_buckets_hot"),
    ("balance-hot-region-scheduler", "pending-op-fails"),
    ("balance-hot-region-scheduler", "placement_scope_fallback"),
    (
        "balance-hot-region-scheduler",
        "read-skip-all-dim-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "read-skip-byte-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "read-skip-cpu-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "read-skip-key-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "read-skip-query-uniform-store",
    ),
    ("balance-hot-region-scheduler", "region_buckets_not_hot"),
    (
        "balance-hot-region-scheduler",
        "region_buckets_single_hot_spot",
    ),
    (
        "balance-hot-region-scheduler",
        "region_is_too_hot_need_split",
    ),
    ("balance-hot-region-scheduler", "schedule"),
    ("balance-hot-region-scheduler", "search_revert_regions"),
    ("balance-hot-region-scheduler", "skip"),
    ("balance-hot-region-scheduler", "snapshot_sender_limit"),
    ("balance-hot-region-scheduler", "split_success"),
    ("balance-hot-region-scheduler", "unhealthy_replica"),
    (
        "balance-hot-region-scheduler",
        "write-skip-all-dim-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "write-skip-byte-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "write-skip-key-uniform-store",
    ),
    (
        "balance-hot-region-scheduler",
        "write-skip-query-uniform-store",
    ),
    ("balance-leader-scheduler", "new-operator"),
    ("balance-leader-scheduler", "no-follower-region"),
    ("balance-leader-scheduler", "no-leader-region"),
    ("balance-leader-scheduler", "no-source-store"),
    ("balance-leader-scheduler", "no-target-store"),
    ("balance-leader-scheduler", "region-hot"),
    ("balance-leader-scheduler", "schedule"),
    ("balance-leader-scheduler", "skip"),
    ("balance-range-scheduler", "balanced"),
    ("balance-range-scheduler", "create-operator-fail"),
    ("balance-range-scheduler", "expired"),
    ("balance-range-scheduler", "new-operator"),
    ("balance-range-scheduler", "no-job"),
    ("balance-range-scheduler", "no-leader"),
    ("balance-range-scheduler", "no-region"),
    ("balance-range-scheduler", "no-replacement"),
    ("balance-range-scheduler", "persist-failed"),
    ("balance-range-scheduler", "region-hot"),
    ("balance-range-scheduler", "schedule"),
    ("balance-region-scheduler", "create-operator-fail"),
    ("balance-region-scheduler", "new-operator"),
    ("balance-region-scheduler", "no-leader"),
    ("balance-region-scheduler", "no-region"),
    ("balance-region-scheduler", "no-replacement"),
    ("balance-region-scheduler", "region-hot"),
    ("balance-region-scheduler", "schedule"),
    ("balance-region-scheduler", "skip"),
    ("evict-leader-scheduler", "new-operator"),
    ("evict-leader-scheduler", "no-leader"),
    ("evict-leader-scheduler", "no-target-store"),
    ("evict-leader-scheduler", "pick-unhealthy-region"),
    ("evict-leader-scheduler", "schedule"),
    ("evict-slow-store-scheduler", "schedule"),
    ("evict-stopping-store-scheduler", "schedule"),
    ("grant-hot-region-scheduler", "new-operator"),
    ("grant-hot-region-scheduler", "no-follower"),
    ("grant-hot-region-scheduler", "schedule"),
    ("grant-hot-region-scheduler", "skip"),
    ("label-scheduler", "new-operator"),
    ("label-scheduler", "no-region"),
    ("label-scheduler", "no-target"),
    ("label-scheduler", "schedule"),
    ("label-scheduler", "skip"),
    ("random-merge-scheduler", "new-operator"),
    ("random-merge-scheduler", "no-region"),
    ("random-merge-scheduler", "no-source-store"),
    ("random-merge-scheduler", "no-target-store"),
    ("random-merge-scheduler", "not-allowed"),
    ("random-merge-scheduler", "schedule"),
    ("scatter-range-scheduler", "new-leader-operator"),
    ("scatter-range-scheduler", "new-operator"),
    ("scatter-range-scheduler", "new-region-operator"),
    ("scatter-range-scheduler", "no-need-balance-leader"),
    ("scatter-range-scheduler", "no-need-balance-region"),
    ("scatter-range-scheduler", "schedule"),
    ("shuffle-hot-region-scheduler", "new-operator"),
    ("shuffle-hot-region-scheduler", "schedule"),
    ("shuffle-hot-region-scheduler", "skip"),
    ("shuffle-leader-scheduler", "new-operator"),
    ("shuffle-leader-scheduler", "no-follower"),
    ("shuffle-leader-scheduler", "no-target-store"),
    ("shuffle-leader-scheduler", "schedule"),
    ("shuffle-region-scheduler", "create-operator-fail"),
    ("shuffle-region-scheduler", "new-operator"),
    ("shuffle-region-scheduler", "no-new-peer"),
    ("shuffle-region-scheduler", "no-region"),
    ("shuffle-region-scheduler", "no-source-store"),
    ("shuffle-region-scheduler", "schedule"),
    ("transfer-witness-leader-scheduler", "new-operator"),
    ("transfer-witness-leader-scheduler", "no-target-store"),
    ("transfer-witness-leader-scheduler", "schedule"),
];
#[derive(Clone)]
enum Vector {
    Counter(CounterVec),
    Gauge(GaugeVec),
    Histogram(HistogramVec),
}
impl Vector {
    fn collector(&self) -> Box<dyn Collector> {
        match self {
            Self::Counter(v) => Box::new(v.clone()),
            Self::Gauge(v) => Box::new(v.clone()),
            Self::Histogram(v) => Box::new(v.clone()),
        }
    }
    fn reset(&self) {
        match self {
            Self::Counter(v) => v.reset(),
            Self::Gauge(v) => v.reset(),
            Self::Histogram(v) => v.reset(),
        }
    }
    fn remove(&self, labels: &[&str]) {
        let _ = match self {
            Self::Counter(v) => v.remove_label_values(labels),
            Self::Gauge(v) => v.remove_label_values(labels),
            Self::Histogram(v) => v.remove_label_values(labels),
        };
    }
}
struct Family {
    definition: &'static Definition,
    vector: Vector,
    series: BTreeSet<Vec<String>>,
}
/// Clones share registry series and serialize writes with per-store deletion.
/// No global registry, runtime or background worker is created.
#[derive(Clone)]
pub struct SchedulerMetrics {
    families: Arc<Mutex<BTreeMap<&'static str, Family>>>,
}
impl SchedulerMetrics {
    /// Registration failure rolls back only collectors added by this call.
    pub fn register(registry: &Registry) -> prometheus::Result<Self> {
        let mut families = BTreeMap::new();
        let mut added: Vec<Vector> = vec![];
        for definition in DEFINITIONS {
            let vector = match definition.kind {
                "counter" => Vector::Counter(CounterVec::new(
                    Opts::new(definition.name, definition.help),
                    definition.labels,
                )?),
                "histogram" => Vector::Histogram(HistogramVec::new(
                    HistogramOpts::new(definition.name, definition.help)
                        .buckets((0..30).map(|i| 2_f64.powi(i)).collect()),
                    definition.labels,
                )?),
                _ => Vector::Gauge(GaugeVec::new(
                    Opts::new(definition.name, definition.help),
                    definition.labels,
                )?),
            };
            if let Err(error) = registry.register(vector.collector()) {
                for v in added {
                    registry.unregister(v.collector())?;
                }
                return Err(error);
            }
            added.push(vector.clone());
            families.insert(
                definition.name,
                Family {
                    definition,
                    vector,
                    series: BTreeSet::new(),
                },
            );
        }
        let metrics = Self {
            families: Arc::new(Mutex::new(families)),
        };
        for &(typ, event) in INITIAL_EVENTS {
            // Exactly two static labels; no caller input and no fallible arity.
            metrics
                .event_metrics(typ, event)
                .expect("fixed source event labels");
        }
        Ok(metrics)
    }
    /// Set a gauge, increment a counter, or observe a histogram. Arity/type/value
    /// errors are returned before creating a series. Host adapters supply labels
    /// from actual policy events, never region IDs or error text.
    pub fn record_metrics(
        &self,
        name: &str,
        labels: &[&str],
        value: f64,
    ) -> prometheus::Result<()> {
        let mut families = self.families.lock().unwrap_or_else(|p| p.into_inner());
        let family = families
            .get_mut(name)
            .ok_or_else(|| prometheus::Error::Msg("unknown scheduler metric".into()))?;
        if labels.len() != family.definition.labels.len()
            || !value.is_finite()
            || (family.definition.kind == "counter" && value < 0.)
        {
            return Err(prometheus::Error::Msg(
                "invalid scheduler metric labels or value".into(),
            ));
        }
        match &family.vector {
            Vector::Counter(v) => v.get_metric_with_label_values(labels)?.inc_by(value),
            Vector::Gauge(v) => v.get_metric_with_label_values(labels)?.set(value),
            Vector::Histogram(v) => v.get_metric_with_label_values(labels)?.observe(value),
        }
        family
            .series
            .insert(labels.iter().map(|s| (*s).to_owned()).collect());
        Ok(())
    }
    /// Cache a counter handle for a real policy event. Event counters survive
    /// ResetSchedulerMetrics in Go; keeping this handle across reset is safe.
    pub fn event_metrics(&self, scheduler_type: &str, event: &str) -> prometheus::Result<Counter> {
        let families = self.families.lock().unwrap_or_else(|p| p.into_inner());
        let Vector::Counter(v) = &families["pd_scheduler_event_count"].vector else {
            unreachable!()
        };
        v.get_metric_with_label_values(&[scheduler_type, event])
    }
    /// Preserve the source event-label alias for grant-leader and instance names
    /// for balance-witness/slow-trend. Raw event_metrics remains available for
    /// source call sites whose explicit labels differ from the policy identity.
    pub fn scheduler_event_metrics(
        &self,
        typ: &astersql_pd_schedule_types::CheckerSchedulerType,
        name: &str,
        event: &str,
    ) -> prometheus::Result<Counter> {
        use astersql_pd_schedule_types::{
            BALANCE_WITNESS_SCHEDULER, EVICT_SLOW_TREND_SCHEDULER, GRANT_LEADER_SCHEDULER,
        };
        // Preserve Go's grantLeaderCounterWithEvent alias and custom-name policies.
        let label = if *typ == GRANT_LEADER_SCHEDULER {
            "grant-hot-region-scheduler"
        } else if *typ == BALANCE_WITNESS_SCHEDULER || *typ == EVICT_SLOW_TREND_SCHEDULER {
            name
        } else {
            typ.as_str()
        };
        self.event_metrics(label, event)
    }
    /// Go's pause/halt status deliberately does not consult policy limits.
    pub fn collect_scheduler_metrics<'a>(
        &self,
        schedulers: impl IntoIterator<Item = (&'a str, bool)>,
        halted: bool,
        rules: Option<&astersql_pd_schedule_placement::RuleManager>,
        labeler: Option<&astersql_pd_schedule_labeler::RegionLabeler>,
        now: astersql_pd_schedule_labeler::Timestamp,
    ) {
        for (name, paused) in schedulers {
            self.record_metrics(
                "pd_scheduler_status",
                &[name, "allow"],
                if !paused && !halted { 1. } else { 0. },
            )
            .expect("fixed status labels");
        }
        if let Some(rules) = rules {
            self.record_metrics(
                "pd_rule_manager_status",
                &["rule_count"],
                rules.get_rules_count() as f64,
            )
            .expect("fixed count labels");
            self.record_metrics(
                "pd_rule_manager_status",
                &["group_count"],
                rules.get_groups_count() as f64,
            )
            .expect("fixed count labels");
        }
        if let Some(labeler) = labeler {
            let (rules, ranges) = labeler.get_rule_and_key_range_counts(now);
            self.record_metrics("pd_region_label_status", &["rule_count"], rules as f64)
                .expect("fixed count labels");
            self.record_metrics("pd_region_label_status", &["range_count"], ranges as f64)
                .expect("fixed count labels");
        }
    }
    pub fn remove_scheduler_metrics(&self, name: &str) {
        let mut families = self.families.lock().unwrap_or_else(|p| p.into_inner());
        let f = families
            .get_mut("pd_scheduler_status")
            .expect("fixed family");
        f.vector.remove(&[name, "allow"]);
        f.series.remove(&vec![name.to_owned(), "allow".into()]);
    }
    /// Preserve Go's intentionally selective reset; event/tolerant/pending/histogram
    /// and trend action/misc/job series survive a scheduler-wide reset.
    pub fn reset_scheduler_metrics(&self) {
        for family in self
            .families
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values_mut()
        {
            if family.definition.reset {
                family.vector.reset();
                family.series.clear();
            }
        }
    }
    pub fn delete_store_metrics(&self, store: &str) {
        for family in self
            .families
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values_mut()
        {
            let witness = family.definition.name == "pd_scheduler_balance_witness";
            if !family.definition.delete_store && !witness {
                continue;
            }
            let index = family
                .definition
                .labels
                .iter()
                .position(|l| *l == "store")
                .expect("store label");
            family.series.retain(|labels| {
                let delete = if witness {
                    labels[0] == "move-witness"
                        && (labels[index] == format!("{store}-in")
                            || labels[index] == format!("{store}-out"))
                } else {
                    labels[index] == store
                };
                if delete {
                    family
                        .vector
                        .remove(&labels.iter().map(String::as_str).collect::<Vec<_>>());
                }
                !delete
            });
        }
    }
    /// Publish real signed pending influence using fresh cluster liveness, then
    /// recheck after writing as Go does to avoid resurrecting buried stores.
    pub fn collect_hot_pending_metrics(
        &self,
        cluster: &impl astersql_pd_schedule_core::BasicCluster,
        stores: &std::collections::HashMap<u64, astersql_pd_statistics::StoreSummaryInfo>,
    ) -> prometheus::Result<()> {
        use astersql_pd_statistics_utils::RWType;
        for (&id, summary) in stores {
            if cluster.get_store(id).is_none_or(|s| s.is_removed()) {
                continue;
            }
            let Some(pending) = &summary.pending_sum else {
                continue;
            };
            if pending.loads.is_empty() {
                continue;
            }
            if pending.loads.len() < astersql_pd_statistics_utils::REGION_STAT_COUNT
                || pending.loads.iter().any(|v| !v.is_finite())
            {
                return Err(prometheus::Error::Msg(
                    "invalid pending load dimensions or value".into(),
                ));
            }
            let label = id.to_string();
            for rw in [RWType::READ, RWType::WRITE] {
                for (dim, name) in ["byte", "key", "query", "cpu"].into_iter().enumerate() {
                    if let Some(kind) = rw.region_stats().get(dim).copied() {
                        if let Some(value) = pending.loads.get(kind.as_i32() as usize) {
                            self.record_metrics(
                                "pd_scheduler_hot_pending_sum",
                                &[&label, &rw.to_string(), name],
                                *value,
                            )?;
                        }
                    }
                }
            }
            if cluster.get_store(id).is_none_or(|s| s.is_removed()) {
                let mut families = self.families.lock().unwrap_or_else(|p| p.into_inner());
                let f = families
                    .get_mut("pd_scheduler_hot_pending_sum")
                    .expect("fixed family");
                f.series.retain(|labels| {
                    if labels[0] == label {
                        f.vector
                            .remove(&labels.iter().map(String::as_str).collect::<Vec<_>>());
                        false
                    } else {
                        true
                    }
                });
            }
        }
        Ok(())
    }
}

/// Optional lifecycle adapter for the canonical loop. A cached event counter
/// records only actual policy calls, including empty retries and error returns.
/// Operator admission counters remain the host admission boundary's responsibility.
pub struct MetricsScheduler<S> {
    pub scheduler: S,
    metrics: SchedulerMetrics,
    name: String,
    schedule: Counter,
}
impl<S> MetricsScheduler<S> {
    pub fn new(
        scheduler: S,
        metrics: SchedulerMetrics,
        typ: &astersql_pd_schedule_types::CheckerSchedulerType,
        name: String,
    ) -> prometheus::Result<Self> {
        let schedule = metrics.scheduler_event_metrics(typ, &name, "schedule")?;
        Ok(Self {
            scheduler,
            metrics,
            name,
            schedule,
        })
    }
}
impl<S> Drop for MetricsScheduler<S> {
    fn drop(&mut self) {
        self.metrics.remove_scheduler_metrics(&self.name);
    }
}
impl<S: crate::LoopScheduler> crate::LoopScheduler for MetricsScheduler<S> {
    type Error = S::Error;
    type Config = S::Config;
    fn admission_config(&self) -> &Self::Config {
        self.scheduler.admission_config()
    }
    fn waiting_random(&mut self) -> f64 {
        self.scheduler.waiting_random()
    }
    fn min_interval(&self) -> i64 {
        self.scheduler.min_interval()
    }
    fn next_interval(&self, current: i64) -> i64 {
        self.scheduler.next_interval(current)
    }
    fn allowed(&self) -> bool {
        self.scheduler.allowed()
    }
    fn evict_leader(&self) -> bool {
        self.scheduler.evict_leader()
    }
    async fn schedule(
        &mut self,
        cluster: &crate::CacheCluster<'_>,
        diagnostic: bool,
    ) -> Result<Vec<Arc<astersql_pd_schedule_operator::Operator>>, Self::Error> {
        self.schedule.inc();
        self.scheduler.schedule(cluster, diagnostic).await
    }
    async fn clean(&mut self, cluster: &astersql_pd_core::BasicCluster) -> Result<(), Self::Error> {
        let result = self.scheduler.clean(cluster).await;
        self.metrics.remove_scheduler_metrics(&self.name);
        result
    }
}
