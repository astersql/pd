// Copyright 2026 AsterSQL.
//! Statistics collectors registered only with a host-owned Prometheus registry.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

use astersql_pd_core::StoreInfo;
use astersql_pd_statistics_utils::StoreLoadKind;
use prometheus::{
    Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts, Registry, core::Collector,
};

use crate::{
    HotCache, HotPeerCache, HotTask, HotTaskError, HotTaskOutput, LabelStatistics,
    RegionAnomalyDurations, RegionStatisticType, RegionStatistics, StoreMetricSample, StoresStats,
};

const REGION_TYPES: [(RegionStatisticType, &str); 10] = [
    (RegionStatisticType::MissPeer, "miss-peer-region-count"),
    (RegionStatisticType::ExtraPeer, "extra-peer-region-count"),
    (RegionStatisticType::DownPeer, "down-peer-region-count"),
    (
        RegionStatisticType::PendingPeer,
        "pending-peer-region-count",
    ),
    (
        RegionStatisticType::OfflinePeer,
        "offline-peer-region-count",
    ),
    (
        RegionStatisticType::LearnerPeer,
        "learner-peer-region-count",
    ),
    (RegionStatisticType::EmptyRegion, "empty-region-count"),
    (
        RegionStatisticType::OversizedRegion,
        "oversized-region-count",
    ),
    (
        RegionStatisticType::UndersizedRegion,
        "undersized-region-count",
    ),
    (
        RegionStatisticType::WitnessLeader,
        "witness-leader-region-count",
    ),
];
const STORE_RESOURCES: [&str; 6] = [
    "region_count",
    "leader_count",
    "witness_count",
    "learner_count",
    "storage_size",
    "storage_capacity",
];
const STORE_STATUSES: [&str; 12] = [
    "store_up_count",
    "store_disconnected_count",
    "store_slow_count",
    "store_down_count",
    "store_unhealth_count",
    "store_offline_count",
    "store_tombstone_count",
    "store_low_space_count",
    "store_preparing_count",
    "store_serving_count",
    "store_removing_count",
    "store_removed_count",
];
type GaugeKey = (&'static str, Vec<String>);

/// A host-scoped metric set. Cache handles under the same lock as deletion so a
/// subsequent collection can always recreate a removed series. No global registry
/// or runtime is installed. The host serializes collection/reset event ordering.
pub struct StatisticsMetrics {
    gauges: BTreeMap<&'static str, GaugeVec>,
    handles: Mutex<BTreeMap<GaugeKey, Gauge>>,
    miss_voter_duration: Histogram,
    down_peer_duration: Histogram,
}

impl StatisticsMetrics {
    /// Registers the complete Go statistics metric family set. A duplicate or
    /// conflicting collector returns an error and rolls back this attempt only.
    pub fn register(registry: &Registry) -> prometheus::Result<Self> {
        let mut gauges = BTreeMap::new();
        let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
        for &(name, help, labels) in GAUGES {
            let gauge = GaugeVec::new(Opts::new(name, help), labels)?;
            collectors.push(Box::new(gauge.clone()));
            gauges.insert(name, gauge);
        }
        let histogram = HistogramVec::new(
            HistogramOpts::new(
                "pd_regions_abnormal_peer_duration_seconds",
                "Bucketed histogram of processing time (s) of handled success cmds.",
            )
            .buckets(prometheus::exponential_buckets(1., 1.4, 30)?),
            &["type"],
        )?;
        collectors.push(Box::new(histogram.clone()));
        for registered in 0..collectors.len() {
            // Clones share values, but each registry owns its registration.
            let owned: Box<dyn Collector> = if registered < GAUGES.len() {
                Box::new(gauges[GAUGES[registered].0].clone())
            } else {
                Box::new(histogram.clone())
            };
            if let Err(error) = registry.register(owned) {
                for previous in collectors.into_iter().take(registered) {
                    let _ = registry.unregister(previous);
                }
                return Err(error);
            }
        }
        let result = Self {
            gauges,
            handles: Mutex::new(BTreeMap::new()),
            miss_voter_duration: histogram.with_label_values(&["miss-voter-peer"]),
            down_peer_duration: histogram.with_label_values(&["down-peer"]),
        };
        result.reset_region_stats_metrics();
        Ok(result)
    }

    /// Sets one gauge using the Go declaration's label order. Unknown families or
    /// wrong label counts return errors without publishing a partial sample.
    pub fn set_gauge(&self, family: &str, labels: &[&str], value: f64) -> prometheus::Result<()> {
        let (&name, gauge) = self
            .gauges
            .get_key_value(family)
            .ok_or_else(|| prometheus::Error::Msg("unknown statistics gauge family".into()))?;
        let key = (name, labels.iter().map(|s| (*s).to_owned()).collect());
        let mut handles = self
            .handles
            .lock()
            .expect("statistics metric lock poisoned");
        if let Some(handle) = handles.get(&key) {
            handle.set(value);
        } else {
            let handle = gauge.get_metric_with_label_values(labels)?;
            handle.set(value);
            handles.insert(key, handle);
        }
        Ok(())
    }

    /// Publishes canonical StoreStatistics samples. Each collect replaces the
    /// placement census, while all other gauge series keep Go's last value.
    pub fn collect_store_statistics(
        &self,
        samples: &[StoreMetricSample],
    ) -> prometheus::Result<()> {
        self.delete_where(|family, _| family == "pd_cluster_placement_status");
        for sample in samples {
            self.set_gauge(
                sample.family,
                &sample.labels.iter().map(String::as_str).collect::<Vec<_>>(),
                sample.value,
            )?;
        }
        Ok(())
    }

    /// Emits canonical rolling flow rates, skipping removed stores even when an
    /// in-flight heartbeat recreated their rolling history after burial.
    pub fn observe_hot_stat(&self, store: &StoreInfo, stats: &StoresStats) {
        if store.is_removed() {
            return;
        }
        let Some(stats) = stats.get_rolling_store_stats(store.id()) else {
            return;
        };
        let id = store.id().to_string();
        for (kind, typ) in FLOW_TYPES {
            self.set_known(
                "pd_scheduler_store_status",
                &[store.address(), &id, typ],
                stats.get_load(kind),
            );
        }
        for &(kind, typ) in &FLOW_TYPES[..7] {
            self.set_known(
                "pd_scheduler_store_status",
                &[store.address(), &id, &format!("{typ}_instant")],
                stats.get_instant_load(kind),
            );
        }
        for &(kind, typ) in &FLOW_TYPES[10..] {
            self.set_known(
                "pd_scheduler_store_status",
                &[store.address(), &id, &format!("{typ}_instant")],
                stats.get_instant_load(kind),
            );
        }
    }

    /// Collects all ten classification counts using the real region index.
    pub fn collect_region_statistics(&self, stats: &RegionStatistics) {
        for (kind, label) in REGION_TYPES {
            self.set_known(
                "pd_regions_status",
                &[label],
                stats.region_ids_by_type(kind).len() as f64,
            );
        }
    }
    /// Records the cumulative duration emitted by an actual region observation.
    pub fn observe_region_durations(&self, durations: &RegionAnomalyDurations) {
        if let Some(seconds) = durations.miss_voter_seconds {
            self.miss_voter_duration.observe(seconds as f64);
        }
        if let Some(seconds) = durations.down_peer_seconds {
            self.down_peer_duration.observe(seconds as f64);
        }
    }
    /// Publishes label isolation counts; reset is a separate lifecycle event.
    pub fn collect_label_statistics(&self, stats: &LabelStatistics) {
        for (label, count) in stats.label_counter() {
            self.set_known("pd_regions_label_level", &[&label], count as f64);
        }
    }

    /// Deletes only health/lifecycle status for the specified store and engine.
    pub fn delete_cluster_status_metrics(&self, store: &StoreInfo) {
        let id = store.id().to_string();
        self.delete_where(|family, labels| {
            family == "pd_cluster_status"
                && labels[1] == store.engine()
                && labels[2] == id
                && STORE_STATUSES.contains(&labels[0].as_str())
        });
    }
    /// Clears old addresses, arbitrary placement labels, resource samples for both
    /// engines, and all three limits. Health/lifecycle series stay visible.
    pub fn reset_store_statistics(&self, store: &str) {
        self.delete_where(|family, labels| match family {
            "pd_scheduler_store_status" => labels[1] == store,
            "pd_cluster_placement_status" => labels[2] == store,
            "pd_cluster_status" => {
                labels[2] == store
                    && ["tikv", "tiflash"].contains(&labels[1].as_str())
                    && STORE_RESOURCES.contains(&labels[0].as_str())
            }
            "pd_cluster_store_limit" => {
                labels[0] == store
                    && ["add-peer", "remove-peer", "transfer-leader-in"]
                        .contains(&labels[1].as_str())
            }
            _ => false,
        });
    }
    /// Keeps the ten fixed series registered and sets their values to zero.
    pub fn reset_region_stats_metrics(&self) {
        for (_, label) in REGION_TYPES {
            self.set_known("pd_regions_status", &[label], 0.);
        }
    }
    pub fn reset_label_stats_metrics(&self) {
        self.delete_where(|family, _| family == "pd_regions_label_level");
    }
    pub fn reset_hot_cache_status_metrics(&self) {
        self.delete_where(|family, _| family == "pd_hotcache_status");
    }
    /// Resets the same families as Go Reset; config, summary and durations survive.
    pub fn reset(&self) {
        self.delete_where(|family, _| {
            matches!(
                family,
                "pd_scheduler_store_status"
                    | "pd_cluster_placement_status"
                    | "pd_cluster_status"
                    | "pd_cluster_store_limit"
                    | "pd_regions_label_level"
                    | "pd_hotcache_status"
            )
        });
        self.reset_region_stats_metrics();
    }
    fn set_known(&self, family: &str, labels: &[&str], value: f64) {
        self.set_gauge(family, labels, value)
            .expect("valid statistics metric declaration");
    }
    fn delete_where(&self, mut predicate: impl FnMut(&str, &[String]) -> bool) {
        self.handles
            .lock()
            .expect("statistics metric lock poisoned")
            .retain(|(family, labels), _| {
                if !predicate(family, labels) {
                    return true;
                }
                self.gauges[family]
                    .remove_label_values(labels)
                    .expect("cached metric exists");
                false
            });
    }
}

const FLOW_TYPES: [(StoreLoadKind, &str); 12] = [
    (StoreLoadKind::WRITE_BYTES, "store_write_rate_bytes"),
    (StoreLoadKind::READ_BYTES, "store_read_rate_bytes"),
    (StoreLoadKind::WRITE_KEYS, "store_write_rate_keys"),
    (StoreLoadKind::READ_KEYS, "store_read_rate_keys"),
    (StoreLoadKind::WRITE_QUERY, "store_write_query_rate"),
    (StoreLoadKind::READ_QUERY, "store_read_query_rate"),
    (StoreLoadKind::READ_CPU, "store_read_cpu_usage"),
    (StoreLoadKind::CPU_USAGE, "store_cpu_usage"),
    (StoreLoadKind::DISK_READ_RATE, "store_disk_read_rate"),
    (StoreLoadKind::DISK_WRITE_RATE, "store_disk_write_rate"),
    (
        StoreLoadKind::REGIONS_WRITE_BYTES,
        "store_regions_write_rate_bytes",
    ),
    (
        StoreLoadKind::REGIONS_WRITE_KEYS,
        "store_regions_write_rate_keys",
    ),
];

const GAUGES: &[(&str, &str, &[&str])] = &[
    (
        "pd_hotcache_status",
        "Status of the hotspot.",
        &["name", "store", "type"],
    ),
    (
        "pd_scheduler_store_status",
        "Store status for schedule",
        &["address", "store", "type"],
    ),
    ("pd_regions_status", "Status of the regions.", &["type"]),
    (
        "pd_cluster_status",
        "Status of the cluster.",
        &["type", "engine", "store"],
    ),
    (
        "pd_cluster_placement_status",
        "Status of the cluster placement.",
        &["type", "name", "store"],
    ),
    (
        "pd_config_status",
        "Status of the scheduling configurations.",
        &["type"],
    ),
    (
        "pd_cluster_store_limit",
        "Status of the store limit.",
        &["store", "type"],
    ),
    (
        "pd_regions_label_level",
        "Number of regions in the different label level.",
        &["type"],
    ),
    (
        "pd_scheduler_hot_peers_summary",
        "Hot peers summary for each store",
        &["type", "store"],
    ),
];

/// Complete owned metric collection from one directional worker. A later
/// collection repairs stale labels even if an earlier worker result was dropped.
#[derive(Clone, Debug)]
pub struct HotMetricsSnapshot {
    kind: astersql_pd_statistics_utils::RWType,
    samples: Vec<StoreMetricSample>,
}
impl HotPeerCache {
    /// Collects thresholds/actions, then performs the normal throttled GC tick.
    /// The host supplies every live store, including stores with no hot peers.
    pub fn collect_metrics(&mut self, now: i64, active: &BTreeSet<u64>) -> HotMetricsSnapshot {
        let mut samples = vec![];
        let kind = self.kind.to_string();
        {
            let thresholds = self
                .hot_thresholds
                .snapshots
                .lock()
                .expect("threshold snapshot lock poisoned");
            for (&store, snapshot) in thresholds.iter() {
                for (name, value) in [
                    "byte-rate-threshold",
                    "key-rate-threshold",
                    "query-rate-threshold",
                    "cpu-rate-threshold",
                    "total_length",
                ]
                .into_iter()
                .zip(
                    snapshot
                        .rates
                        .into_iter()
                        .chain([snapshot.top_n_len as f64]),
                ) {
                    samples.push(StoreMetricSample {
                        family: "pd_hotcache_status",
                        labels: vec![name.into(), format!("store-{store}"), kind.clone()],
                        value,
                    });
                }
            }
        }
        for (&store, counts) in &self.metric_actions {
            for (name, &value) in ["add_item", "remove_item", "update_item"]
                .into_iter()
                .zip(counts)
            {
                samples.push(StoreMetricSample {
                    family: "pd_hotcache_status",
                    labels: vec![name.into(), format!("store-{store}"), kind.clone()],
                    value,
                });
            }
        }
        self.gc(now, active);
        // GC can remove a store without any new report, including an explicit
        // host GC before this tick. Only publish stores still represented in the
        // actual cache after the tick; retained threshold snapshots keep Go's
        // last total_length until the next threshold calculation.
        let thresholds = self
            .hot_thresholds
            .snapshots
            .lock()
            .expect("threshold snapshot lock poisoned");
        let retained: BTreeSet<_> = self
            .metric_actions
            .keys()
            .chain(thresholds.keys())
            .map(|id| format!("store-{id}"))
            .collect();
        samples.retain(|sample| retained.contains(&sample.labels[1]));
        HotMetricsSnapshot {
            kind: self.kind,
            samples,
        }
    }
}
impl StatisticsMetrics {
    /// Reconciles one complete directional collection; this never mutates the cache.
    pub fn collect_hot_cache_metrics(&self, snapshot: &HotMetricsSnapshot) {
        let kind = snapshot.kind.to_string();
        let current: BTreeSet<_> = snapshot
            .samples
            .iter()
            .map(|sample| sample.labels.as_slice())
            .collect();
        // A complete directional snapshot is authoritative. Reconciliation does
        // not consume one-shot removal events, so dropped receipts cannot leave
        // stale store labels behind, and reactivated stores lose old thresholds.
        self.delete_where(|family, labels| {
            family == "pd_hotcache_status" && labels[2] == kind && !current.contains(labels)
        });
        for sample in &snapshot.samples {
            self.set_known(
                sample.family,
                &sample.labels.iter().map(String::as_str).collect::<Vec<_>>(),
                sample.value,
            );
        }
    }
}
impl HotCache {
    /// Drives collection/GC on both existing workers, even without new reports.
    /// Partial admission/execution errors are returned after both queues are
    /// handled. The host retains its usual shutdown/cancel/join responsibility.
    pub async fn collect_metrics(
        &self,
        metrics: &StatisticsMetrics,
        now: i64,
        active_stores: &BTreeSet<u64>,
    ) -> Result<(), HotTaskError> {
        let mut error = None;
        for kind in [
            astersql_pd_statistics_utils::RWType::WRITE,
            astersql_pd_statistics_utils::RWType::READ,
        ] {
            let result = match self.submit(
                kind,
                HotTask::Metrics {
                    active_stores: active_stores.clone(),
                },
                now,
            ) {
                Ok(receipt) => receipt.wait().await,
                Err(error) => Err(error),
            };
            match result {
                Ok(HotTaskOutput::Metrics(snapshot)) => {
                    metrics.collect_hot_cache_metrics(&snapshot)
                }
                Ok(_) => unreachable!("metrics task returns metrics snapshot"),
                Err(next) => {
                    error.get_or_insert(next);
                }
            }
        }
        error.map_or(Ok(()), Err)
    }
}
