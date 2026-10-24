// Copyright 2026 AsterSQL.
//! Store health and placement samples, independent of a process Registry.

use std::collections::BTreeMap;

use astersql_pd_core::{NodeState, StoreInfo, StoreLimiter};
use astersql_pd_schedule_config::ConfProvider;

/// One gauge value with labels in the corresponding Go metric's declared order.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreMetricSample {
    pub family: &'static str,
    pub labels: Vec<String>,
    pub value: f64,
}

type SampleKey = (&'static str, Vec<String>);

/// One collection's placement membership and last observed per-store samples.
/// Repeated observations append placement membership, matching Go. The host
/// starts a new collection for a fresh placement census; no global state is used.
#[derive(Default)]
pub struct StoreStatistics {
    pub label_counter: BTreeMap<String, Vec<u64>>,
    samples: BTreeMap<SampleKey, f64>,
}
impl StoreStatistics {
    /// Compute all twelve status values using the host's Unix nanosecond clock.
    /// Down/unhealthy/disconnected/slow/up are mutually exclusive; lifecycle
    /// and low-space values are independent except that down suppresses space.
    pub fn observe_store_status<P: ConfProvider>(
        store: &StoreInfo,
        opt: &P,
        now: i128,
    ) -> BTreeMap<String, f64> {
        let mut result: BTreeMap<_, _> = [
            "up",
            "disconnected",
            "slow",
            "down",
            "unhealth",
            "offline",
            "tombstone",
            "low_space",
            "preparing",
            "serving",
            "removing",
            "removed",
        ]
        .into_iter()
        .map(|s| (format!("store_{s}_count"), 0.0))
        .collect();
        let mut down = false;
        match store.node_state() {
            NodeState::Preparing | NodeState::Serving => {
                down = store.down_time_nanos(now) >= opt.get_max_store_down_time().as_nanos();
                let health = if down {
                    "down"
                } else if store.is_unhealthy(now) {
                    "unhealth"
                } else if store.is_disconnected(now) {
                    "disconnected"
                } else if store.is_slow() {
                    "slow"
                } else {
                    "up"
                };
                result.insert(format!("store_{health}_count"), 1.0);
                result.insert(
                    if store.is_preparing() {
                        "store_preparing_count"
                    } else {
                        "store_serving_count"
                    }
                    .into(),
                    1.0,
                );
            }
            NodeState::Removing => {
                result.insert("store_offline_count".into(), 1.0);
                result.insert("store_removing_count".into(), 1.0);
            }
            NodeState::Removed => {
                result.insert("store_tombstone_count".into(), 1.0);
                result.insert("store_removed_count".into(), 1.0);
                return result;
            }
            NodeState::Unknown(_) => {}
        }
        if !down && store.is_low_space(opt.get_low_space_ratio()) {
            result.insert("store_low_space_count".into(), 1.0);
        }
        result
    }

    /// Observe one real core store. Removed stores refresh health only, retaining
    /// earlier resource samples until the host applies metric cleanup.
    pub fn observe<P: ConfProvider>(&mut self, store: &StoreInfo, opt: &P, now: i128) {
        if !store.is_removed() {
            for key in opt.get_location_labels() {
                let value = store.label_value(key);
                self.label_counter
                    .entry(format!(
                        "{key}:{}",
                        if value.is_empty() { "unknown" } else { value }
                    ))
                    .or_default()
                    .push(store.id());
            }
        }
        let id = store.id().to_string();
        for (typ, value) in Self::observe_store_status(store, opt, now) {
            self.samples.insert(
                (
                    "pd_cluster_status",
                    vec![typ, store.engine().into(), id.clone()],
                ),
                value,
            );
        }
        if store.is_removed() {
            return;
        }
        for (typ, value) in [
            ("storage_size", store.storage_size() as f64),
            ("storage_capacity", store.capacity() as f64),
            ("region_count", store.region_count() as f64),
            ("leader_count", store.leader_count() as f64),
            ("witness_count", store.witness_count() as f64),
            ("learner_count", store.learner_count() as f64),
        ] {
            self.samples.insert(
                (
                    "pd_cluster_status",
                    vec![typ.into(), store.engine().into(), id.clone()],
                ),
                value,
            );
        }
        let mut values = vec![
            (
                "region_score".to_string(),
                store.region_score(
                    opt.get_region_score_formula_version(),
                    opt.get_high_space_ratio(),
                    opt.get_low_space_ratio(),
                    0,
                ),
            ),
            (
                "leader_score".into(),
                store.leader_score(opt.get_leader_schedule_policy(), 0),
            ),
            ("region_size".into(), store.region_size() as f64),
            ("region_count".into(), store.region_count() as f64),
            ("leader_size".into(), store.leader_size() as f64),
            ("leader_count".into(), store.leader_count() as f64),
            ("witness_count".into(), store.witness_count() as f64),
            ("learner_count".into(), store.learner_count() as f64),
            ("store_available".into(), store.available() as f64),
            ("store_used".into(), store.used_size() as f64),
            ("store_capacity".into(), store.capacity() as f64),
        ];
        if let Some(limiter) = store.limiter() {
            if let StoreLimiter::V2(limit) = &*limiter.read().unwrap() {
                values.push(("windows_size".into(), limit.capacity() as f64));
                for (priority, used) in ["low", "medium", "high", "urgent"]
                    .into_iter()
                    .zip(limit.used())
                {
                    values.push((format!("windows_used_level_{priority}"), used as f64));
                }
            }
        }
        if let Some(trend) = store.store_stats().slow_trend {
            values.extend([
                ("store_slow_trend_cause_value".into(), trend.cause_value),
                ("store_slow_trend_cause_rate".into(), trend.cause_rate),
                ("store_slow_trend_result_value".into(), trend.result_value),
                ("store_slow_trend_result_rate".into(), trend.result_rate),
            ]);
        }
        for (typ, value) in values {
            self.samples.insert(
                (
                    "pd_scheduler_store_status",
                    vec![store.address().into(), id.clone(), typ],
                ),
                value,
            );
        }
    }

    /// Return an owned snapshot, adding current configuration, placement and
    /// explicit store-limit values. Registry registration and deletion are host work.
    pub fn collect<P: ConfProvider>(&self, opt: &P) -> Vec<StoreMetricSample> {
        let mut samples = self.samples.clone();
        let config = [
            (
                "leader-schedule-limit",
                opt.get_leader_schedule_limit() as f64,
            ),
            (
                "region-schedule-limit",
                opt.get_region_schedule_limit() as f64,
            ),
            (
                "merge-schedule-limit",
                opt.get_merge_schedule_limit() as f64,
            ),
            (
                "replica-schedule-limit",
                opt.get_replica_schedule_limit() as f64,
            ),
            (
                "affinity-schedule-limit",
                opt.get_affinity_schedule_limit() as f64,
            ),
            (
                "split-scatter-schedule-limit",
                opt.get_split_scatter_schedule_limit() as f64,
            ),
            ("max-replicas", opt.get_max_replicas() as f64),
            ("high-space-ratio", opt.get_high_space_ratio()),
            ("low-space-ratio", opt.get_low_space_ratio()),
            ("tolerant-size-ratio", opt.get_tolerant_size_ratio()),
            (
                "hot-region-schedule-limit",
                opt.get_hot_region_schedule_limit() as f64,
            ),
            (
                "hot-region-cache-hits-threshold",
                opt.get_hot_region_cache_hits_threshold() as f64,
            ),
            (
                "max-pending-peer-count",
                opt.get_max_pending_peer_count() as f64,
            ),
            ("max-snapshot-count", opt.get_max_snapshot_count() as f64),
            (
                "max-merge-region-size",
                opt.get_max_merge_region_size() as f64,
            ),
            (
                "max-merge-region-keys",
                opt.get_max_merge_region_keys() as f64,
            ),
            (
                "max-affinity-merge-region-size",
                opt.get_max_affinity_merge_region_size() as f64,
            ),
            ("region-max-size", opt.get_region_max_size() as f64),
            ("region-split-size", opt.get_region_split_size() as f64),
            ("region-split-keys", opt.get_region_split_keys() as f64),
            ("region-max-keys", opt.get_region_max_keys() as f64),
            (
                "enable-makeup-replica",
                f64::from(opt.is_make_up_replica_enabled()),
            ),
            (
                "enable-remove-down-replica",
                f64::from(opt.is_remove_down_replica_enabled()),
            ),
            (
                "enable-remove-extra-replica",
                f64::from(opt.is_remove_extra_replica_enabled()),
            ),
            (
                "enable-replace-offline-replica",
                f64::from(opt.is_replace_offline_replica_enabled()),
            ),
        ];
        for (typ, value) in config {
            samples.insert(("pd_config_status", vec![typ.into()]), value);
        }
        for (name, stores) in &self.label_counter {
            for id in stores {
                samples.insert(
                    (
                        "pd_cluster_placement_status",
                        vec!["label".into(), name.clone(), id.to_string()],
                    ),
                    1.0,
                );
            }
        }
        if let Some(limits) = opt.get_stores_limit() {
            for (id, limit) in limits {
                for (typ, value) in [
                    ("add-peer", limit.add_peer),
                    ("remove-peer", limit.remove_peer),
                    ("transfer-leader-in", limit.transfer_leader_in),
                ] {
                    samples.insert(
                        ("pd_cluster_store_limit", vec![id.to_string(), typ.into()]),
                        value,
                    );
                }
            }
        }
        samples
            .into_iter()
            .map(|((family, labels), value)| StoreMetricSample {
                family,
                labels,
                value,
            })
            .collect()
    }
}

/// A host-owned configuration view and its independent store census.
pub struct StoreStatisticsMap<'a, P: ConfProvider> {
    pub stats: StoreStatistics,
    opt: &'a P,
}
impl<'a, P: ConfProvider> StoreStatisticsMap<'a, P> {
    pub fn new(opt: &'a P) -> Self {
        Self {
            stats: StoreStatistics::default(),
            opt,
        }
    }
    pub fn observe(&mut self, store: &StoreInfo, now: i128) {
        self.stats.observe(store, self.opt, now);
    }
    pub fn collect(&self) -> Vec<StoreMetricSample> {
        self.stats.collect(self.opt)
    }
}
