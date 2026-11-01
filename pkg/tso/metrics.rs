// Copyright 2026 AsterSQL.
//! Cached classic-oracle handles owned by the embedding listener.
use prometheus::{Counter, Gauge, Histogram};
use std::collections::BTreeMap;
pub struct OracleMetrics {
    events: BTreeMap<&'static str, Counter>,
    pub(crate) physical: Gauge,
    pub(crate) gap: Gauge,
    pub(crate) role: Gauge,
    pub(crate) sync: Histogram,
    pub(crate) reset: Histogram,
    pub(crate) update: Histogram,
}
impl OracleMetrics {
    pub fn new(scope: &astersql_pd_metricutil::ClassicMetrics) -> Self {
        Self {
            events: [
                "sync",
                "skip_sync",
                "sync_ok",
                "err_save_sync_ts",
                "err_lease_reset_ts",
                "err_reset_physical_small_ts",
                "err_reset_logical_small_ts",
                "err_reset_large_ts",
                "err_save_reset_ts",
                "reset_tso_ok",
                "save",
                "slow_save",
                "system_time_slow",
                "skip_save",
                "err_save_update_ts",
                "not_leader_anymore",
                "logical_overflow",
                "exceeded_max_retry",
                "not_allowed_save_timestamp",
                "not_leader",
                "global_tso_sync",
                "global_tso_estimate",
                "global_tso_persist",
                "global_tso_persist_err",
                "precheck_logical_overflow",
            ]
            .into_iter()
            .map(|event| (event, scope.tso_counter.with_label_values(&[event, "0"])))
            .collect(),
            physical: scope.tso_gauge.with_label_values(&["tso", "0"]),
            // Preserve Go's literal group label on this historical series.
            gap: scope.tso_gap.with_label_values(&["group"]),
            role: scope.tso_allocator_role.with_label_values(&["0"]),
            sync: scope.tso_op_duration.with_label_values(&["sync_save", "0"]),
            reset: scope
                .tso_op_duration
                .with_label_values(&["reset_save", "0"]),
            update: scope
                .tso_op_duration
                .with_label_values(&["update_save", "0"]),
        }
    }
    pub(crate) fn event(&self, event: &'static str) {
        self.events[event].inc();
    }
}
impl Drop for OracleMetrics {
    fn drop(&mut self) {
        self.role.set(0.);
    }
}
