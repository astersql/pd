// Copyright 2026 AsterSQL.
//! Go-compatible metric vocabulary scoped to a host-owned Registry.
use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
    core::Collector,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::time::Instant;

/// Metric identities deliberately differ from configuration checker names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckerMetricKind {
    Rule,
    Joint,
    Learner,
    Merge,
    Replica,
    Split,
    SplitScatter,
    Affinity,
}
impl CheckerMetricKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule_checker",
            Self::Joint => "joint_state_checker",
            Self::Learner => "learner_checker",
            Self::Merge => "merge_checker",
            Self::Replica => "replica_checker",
            Self::Split => "split_checker",
            Self::SplitScatter => "split_scatter_checker",
            Self::Affinity => "affinity_checker",
        }
    }
}
/// Frozen, bounded event vocabulary from metrics.go; region/store IDs never become labels.
const EVENTS: &[(CheckerMetricKind, &str)] = &[
    (CheckerMetricKind::Affinity, "abnormal-replica"),
    (CheckerMetricKind::Affinity, "check"),
    (CheckerMetricKind::Affinity, "create-operator-failed"),
    (CheckerMetricKind::Affinity, "group-scheduling-disabled"),
    (CheckerMetricKind::Affinity, "learner-peer-conflict"),
    (CheckerMetricKind::Affinity, "merge-adj-abnormal-peerstore"),
    (CheckerMetricKind::Affinity, "merge-adj-abnormal-replica"),
    (CheckerMetricKind::Affinity, "merge-adj-different-group"),
    (CheckerMetricKind::Affinity, "merge-adj-disallow-merge"),
    (CheckerMetricKind::Affinity, "merge-adj-not-affinity"),
    (CheckerMetricKind::Affinity, "merge-adj-not-exist"),
    (CheckerMetricKind::Affinity, "merge-adj-unhealthy"),
    (CheckerMetricKind::Affinity, "merge-check"),
    (CheckerMetricKind::Affinity, "merge-disabled"),
    (CheckerMetricKind::Affinity, "merge-global-disabled"),
    (CheckerMetricKind::Affinity, "merge-new-operator"),
    (CheckerMetricKind::Affinity, "merge-no-need"),
    (CheckerMetricKind::Affinity, "merge-no-target"),
    (CheckerMetricKind::Affinity, "merge-skip-cached"),
    (CheckerMetricKind::Affinity, "merge-skip-startup"),
    (CheckerMetricKind::Affinity, "merge-target-too-big"),
    (CheckerMetricKind::Affinity, "new-operator"),
    (CheckerMetricKind::Affinity, "paused"),
    (CheckerMetricKind::Affinity, "placement-rules-disabled"),
    (CheckerMetricKind::Affinity, "region-no-leader"),
    (CheckerMetricKind::Affinity, "unhealthy-region"),
    (CheckerMetricKind::Joint, "check"),
    (CheckerMetricKind::Joint, "create-operator-fail"),
    (CheckerMetricKind::Joint, "new-operator"),
    (CheckerMetricKind::Joint, "paused"),
    (CheckerMetricKind::Joint, "transfer-leader"),
    (CheckerMetricKind::Learner, "paused"),
    (CheckerMetricKind::Merge, "abnormal-replica"),
    (CheckerMetricKind::Merge, "adj-abnormal-peerstore"),
    (CheckerMetricKind::Merge, "adj-abnormal-replica"),
    (CheckerMetricKind::Merge, "adj-disallow-merge"),
    (CheckerMetricKind::Merge, "adj-not-exist"),
    (CheckerMetricKind::Merge, "adj-recently-split"),
    (CheckerMetricKind::Merge, "adj-region-hot"),
    (CheckerMetricKind::Merge, "adj-special-peer"),
    (CheckerMetricKind::Merge, "check"),
    (CheckerMetricKind::Merge, "hot-region"),
    (CheckerMetricKind::Merge, "larger-source"),
    (CheckerMetricKind::Merge, "new-operator"),
    (CheckerMetricKind::Merge, "no-leader"),
    (CheckerMetricKind::Merge, "no-need"),
    (CheckerMetricKind::Merge, "no-target"),
    (CheckerMetricKind::Merge, "paused"),
    (CheckerMetricKind::Merge, "recently-split"),
    (CheckerMetricKind::Merge, "recently-start"),
    (CheckerMetricKind::Merge, "split-keys-after-merge"),
    (CheckerMetricKind::Merge, "split-size-after-merge"),
    (CheckerMetricKind::Merge, "target-too-large"),
    (CheckerMetricKind::Merge, "unhealthy-region"),
    (CheckerMetricKind::Replica, "all-right"),
    (CheckerMetricKind::Replica, "check"),
    (CheckerMetricKind::Replica, "create-operator-failed"),
    (CheckerMetricKind::Replica, "new-operator"),
    (CheckerMetricKind::Replica, "no-store-down"),
    (CheckerMetricKind::Replica, "no-store-offline"),
    (CheckerMetricKind::Replica, "no-target-store"),
    (CheckerMetricKind::Replica, "no-worst-peer"),
    (CheckerMetricKind::Replica, "not-better"),
    (CheckerMetricKind::Replica, "paused"),
    (
        CheckerMetricKind::Replica,
        "remove-extra-down-replica-failed",
    ),
    (
        CheckerMetricKind::Replica,
        "remove-extra-offline-replica-failed",
    ),
    (CheckerMetricKind::Replica, "replace-down-replica-failed"),
    (CheckerMetricKind::Replica, "replace-offline-replica-failed"),
    (CheckerMetricKind::Rule, "add-rule-peer"),
    (CheckerMetricKind::Rule, "add-tiflash-learner"),
    (CheckerMetricKind::Rule, "cancel-switch-to-witness"),
    (CheckerMetricKind::Rule, "check"),
    (CheckerMetricKind::Rule, "demote-voter-role"),
    (CheckerMetricKind::Rule, "fix-follower-role"),
    (CheckerMetricKind::Rule, "fix-leader-role"),
    (CheckerMetricKind::Rule, "fix-peer-role"),
    (CheckerMetricKind::Rule, "get-cache"),
    (CheckerMetricKind::Rule, "move-to-better-location"),
    (CheckerMetricKind::Rule, "need-split"),
    (CheckerMetricKind::Rule, "no-new-leader"),
    (CheckerMetricKind::Rule, "no-store-add"),
    (CheckerMetricKind::Rule, "no-store-replace"),
    (CheckerMetricKind::Rule, "no-store-then-try-replace"),
    (CheckerMetricKind::Rule, "not-allow-leader"),
    (CheckerMetricKind::Rule, "paused"),
    (CheckerMetricKind::Rule, "promote-witness"),
    (CheckerMetricKind::Rule, "recently-promote-to-non-witness"),
    (CheckerMetricKind::Rule, "region-no-leader"),
    (CheckerMetricKind::Rule, "remove-orphan-peer"),
    (CheckerMetricKind::Rule, "replace-down"),
    (CheckerMetricKind::Rule, "replace-offline"),
    (CheckerMetricKind::Rule, "replace-orphan-peer"),
    (CheckerMetricKind::Rule, "replace-orphan-peer-no-fit"),
    (CheckerMetricKind::Rule, "set-cache"),
    (CheckerMetricKind::Rule, "set-learner-non-witness"),
    (CheckerMetricKind::Rule, "set-learner-witness"),
    (CheckerMetricKind::Rule, "set-voter-non-witness"),
    (CheckerMetricKind::Rule, "set-voter-witness"),
    (CheckerMetricKind::Rule, "skip-remove-orphan-peer"),
    (CheckerMetricKind::Split, "check"),
    (CheckerMetricKind::Split, "paused"),
    (
        CheckerMetricKind::SplitScatter,
        "dispatch-add-operator-failed",
    ),
    (
        CheckerMetricKind::SplitScatter,
        "dispatch-balanced-read-cpu",
    ),
    (CheckerMetricKind::SplitScatter, "dispatch-disabled"),
    (CheckerMetricKind::SplitScatter, "dispatch-hot-region"),
    (
        CheckerMetricKind::SplitScatter,
        "dispatch-no-operator-needed",
    ),
    (
        CheckerMetricKind::SplitScatter,
        "dispatch-not-fully-replicated",
    ),
    (CheckerMetricKind::SplitScatter, "dispatch-operator-created"),
    (CheckerMetricKind::SplitScatter, "dispatch-region-missing"),
    (CheckerMetricKind::SplitScatter, "dispatch-scatter-failed"),
    (
        CheckerMetricKind::SplitScatter,
        "dispatch-schedule-disabled",
    ),
    (CheckerMetricKind::SplitScatter, "dispatch-schedule-limit"),
    (CheckerMetricKind::SplitScatter, "dispatch-store-limit"),
];
const PHASES: &[&str] = &[
    "wait_for_channel",
    "check_priority",
    "check_pending",
    "dispatch_split_scatter",
    "scan_regions",
    "update_label_stats",
];
/// Clones share collectors, while separate registries have independent series.
#[derive(Clone)]
pub struct CheckerMetrics {
    pub(crate) inner: Arc<CheckerMetricsInner>,
}
pub(crate) struct CheckerMetricsInner {
    events: HashMap<(CheckerMetricKind, &'static str), Counter>,
    checkers: HashMap<CheckerMetricKind, Histogram>,
    phases: HashMap<&'static str, Histogram>,
    pending: Gauge,
    priority: Gauge,
    pub(crate) patrol_time: Gauge,
    pub(crate) channel_size: Gauge,
    pub(crate) split_pending: Gauge,
    pub(crate) expired_attempted: Counter,
    pub(crate) expired_unattempted: Counter,
    pub(crate) dropped: Counter,
}
impl CheckerMetrics {
    /// Register atomically: a collision rolls back only collectors added by this call.
    /// The library returns the Prometheus error to the host instead of panicking globally.
    pub fn register(registry: &Registry) -> prometheus::Result<Self> {
        let events = CounterVec::new(
            Opts::new("pd_checker_event_count", "Counter of checker events."),
            &["type", "name"],
        )?;
        let lists = GaugeVec::new(
            Opts::new(
                "pd_checker_region_list",
                "Number of region about different type.",
            ),
            &["type"],
        )?;
        let patrol_time = Gauge::new(
            "pd_checker_patrol_regions_time",
            "Time spent of patrol checks region.",
        )?;
        let phases = HistogramVec::new(
            HistogramOpts::new(
                "pd_checker_patrol_phase_duration_seconds",
                "Bucketed histogram of duration for patrol phases.",
            ),
            &["phase"],
        )?;
        let checkers = HistogramVec::new(
            HistogramOpts::new(
                "pd_checker_check_region_duration_seconds",
                "Bucketed histogram of duration for checking region in checkers.",
            ),
            &["checker"],
        )?;
        let channel_size = Gauge::new(
            "pd_checker_patrol_region_channel_size",
            "Size of patrol region channel.",
        )?;
        let expired = CounterVec::new(
            Opts::new(
                "pd_checker_split_scatter_pending_expired_total",
                "Counter of expired split-scatter pending entries by whether they were selected for dispatch.",
            ),
            &["attempted"],
        )?;
        let dropped = Counter::new(
            "pd_checker_split_scatter_pending_dropped_total",
            "Counter of split-scatter pending entries dropped by the capacity limit.",
        )?;
        let split_pending = Gauge::new(
            "pd_checker_split_scatter_pending",
            "Number of split-scatter pending entries.",
        )?;
        let collectors: Vec<Box<dyn Collector>> = vec![
            Box::new(events.clone()),
            Box::new(lists.clone()),
            Box::new(patrol_time.clone()),
            Box::new(phases.clone()),
            Box::new(checkers.clone()),
            Box::new(channel_size.clone()),
            Box::new(expired.clone()),
            Box::new(dropped.clone()),
            Box::new(split_pending.clone()),
        ];
        let mut registered = Vec::new();
        for collector in collectors {
            if let Err(error) = registry.register(collector) {
                for previous in registered {
                    registry.unregister(previous)?;
                }
                return Err(error);
            }
            // Retain an independently boxed clone for rollback without touching host collectors.
            registered.push(match registered.len() {
                0 => Box::new(events.clone()) as Box<dyn Collector>,
                1 => Box::new(lists.clone()),
                2 => Box::new(patrol_time.clone()),
                3 => Box::new(phases.clone()),
                4 => Box::new(checkers.clone()),
                5 => Box::new(channel_size.clone()),
                6 => Box::new(expired.clone()),
                7 => Box::new(dropped.clone()),
                _ => Box::new(split_pending.clone()),
            });
        }
        Ok(Self {
            inner: Arc::new(CheckerMetricsInner {
                events: EVENTS
                    .iter()
                    .map(|&(kind, event)| {
                        (
                            (kind, event),
                            events.with_label_values(&[kind.as_str(), event]),
                        )
                    })
                    .collect(),
                checkers: [
                    CheckerMetricKind::Joint,
                    CheckerMetricKind::Split,
                    CheckerMetricKind::Rule,
                    CheckerMetricKind::Learner,
                    CheckerMetricKind::Replica,
                    CheckerMetricKind::Merge,
                    CheckerMetricKind::Affinity,
                ]
                .into_iter()
                .map(|kind| (kind, checkers.with_label_values(&[kind.as_str()])))
                .collect(),
                phases: PHASES
                    .iter()
                    .map(|&phase| (phase, phases.with_label_values(&[phase])))
                    .collect(),
                pending: lists.with_label_values(&["pending_processed_regions"]),
                priority: lists.with_label_values(&["priority_list"]),
                expired_attempted: expired.with_label_values(&["true"]),
                expired_unattempted: expired.with_label_values(&["false"]),
                patrol_time,
                channel_size,
                split_pending,
                dropped,
            }),
        })
    }
    /// Reject unknown events before any series creation; no metric labels come from callers.
    pub fn event(&self, checker: CheckerMetricKind, event: &str) -> prometheus::Result<()> {
        let Some(counter) = self.inner.events.get(&(checker, event)) else {
            return Err(prometheus::Error::Msg("unknown checker event".into()));
        };
        counter.inc();
        Ok(())
    }
    pub(crate) fn inc(&self, checker: CheckerMetricKind, event: &str) {
        self.event(checker, event)
            .expect("frozen checker metric event");
    }
    pub fn observe_checker_metrics(&self, checker: CheckerMetricKind, duration: Duration) {
        if let Some(histogram) = self.inner.checkers.get(&checker) {
            histogram.observe(duration.as_secs_f64());
        }
    }
    pub(crate) fn phase_metrics(&self, phase: &'static str) -> MetricsTimer {
        MetricsTimer {
            histogram: self.inner.phases[phase].clone(),
            start: Instant::now(),
        }
    }
    pub(crate) fn checker_metrics(&self, checker: CheckerMetricKind) -> MetricsTimer {
        MetricsTimer {
            histogram: self.inner.checkers[&checker].clone(),
            start: Instant::now(),
        }
    }
    pub(crate) fn queue_metrics(&self, priority: bool, count: usize) {
        (if priority {
            &self.inner.priority
        } else {
            &self.inner.pending
        })
        .set(count as f64);
    }
}
/// Scope follows each actually entered stage, including early returns and typed failures.
pub(crate) struct MetricsTimer {
    histogram: Histogram,
    start: Instant,
}
impl Drop for MetricsTimer {
    fn drop(&mut self) {
        self.histogram.observe(self.start.elapsed().as_secs_f64());
    }
}
