// Copyright 2026 AsterSQL.
//! Host-serialized split-scatter retry policy over the canonical observer.
use crate::{
    SplitScatterObservation, SplitScatterObserver, SplitScatterPendingItem, SplitScatterRangeHint,
    has_split_scatter_txn_keyspace_bounds, observe_split_scatter_result,
    resolve_split_scatter_range_hint, split_scatter_region_version,
};
use astersql_pd_core::{BasicCluster, RegionInfo, RegionsInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{RegionReplicationContext, is_region_replicated};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{
    AdmissionError, CancelReason, ControllerAdmission, OpKind, Operator,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub const SPLIT_SCATTER_PENDING_LIMIT: usize = 4096;
pub const SPLIT_SCATTER_RETRY_BACKOFF: Duration = Duration::from_secs(10);
pub const SPLIT_SCATTER_PENDING_TTL: Duration = Duration::from_secs(1800);
pub const INTERNAL_SPLIT_SCATTER_DESC: &str = "split-scatter-region";

/// Retry classification returned by the host's scatter planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitScatterRetryReason {
    HotRegion,
    BalancedReadCpu,
    Other(String),
}
/// Planning failures remain visible in dispatch results while pending work is retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitScatterDispatchError {
    RetryLater(SplitScatterRetryReason),
    Planning(String),
    Admission(AdmissionError),
}
impl std::fmt::Display for SplitScatterDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SplitScatterDispatchError {}
/// Per-controller cumulative observations; the host exports these to its metrics registry.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SplitScatterRetryMetrics {
    pub pending: usize,
    pub expired_attempted: u64,
    pub expired_unattempted: u64,
    pub dropped: u64,
    pub missing: u64,
    pub disabled: u64,
    pub schedule_limit: u64,
    pub schedule_disabled: u64,
    pub not_replicated: u64,
    pub hot: u64,
    pub balanced_read_cpu: u64,
    pub scatter_failed: u64,
    pub store_limit: u64,
    pub add_operator_failed: u64,
    pub operator_created: u64,
    pub no_operator_needed: u64,
}
#[derive(Default)]
struct RetryState {
    retry_at: Duration,
    attempted: bool,
}
/// One owner calls methods sequentially on the host runtime. No private task,
/// runtime or global metric registration is installed. Identity lives only in
/// SplitScatterObserver; this map stores policy metadata for those same entries.
#[derive(Default)]
pub struct SplitScatterRetryController {
    registry_metrics: Option<crate::CheckerMetrics>,
    observer: SplitScatterObserver,
    retry: BTreeMap<u64, RetryState>,
    next_dispatch_at: Duration,
    metrics: SplitScatterRetryMetrics,
}
impl SplitScatterRetryController {
    /// Attach before accepting work. Existing pending state is published once.
    /// Counter history remains owned by this controller and is never replayed.
    pub fn set_metrics(&mut self, metrics: crate::CheckerMetrics) {
        metrics.inner.split_pending.set(self.metrics.pending as f64);
        self.registry_metrics = Some(metrics);
    }
    pub fn metrics(&self) -> &SplitScatterRetryMetrics {
        &self.metrics
    }
    pub fn pending(&self, id: u64) -> Option<SplitScatterPendingItem> {
        self.observer.pending(id)
    }
    pub fn retry_at(&self, id: u64) -> Option<Duration> {
        self.retry.get(&id).map(|r| r.retry_at)
    }
    pub fn next_dispatch_at(&self) -> Duration {
        self.next_dispatch_at
    }
    pub fn is_current(&self, expected: &SplitScatterPendingItem) -> bool {
        self.observer.is_current(expected)
    }
    pub fn delete(&mut self, expected: &SplitScatterPendingItem) -> bool {
        if !self.observer.remove_if_current(expected) {
            return false;
        }
        self.retry.remove(&expected.region_id);
        self.metrics.pending = self.observer.len();
        if let Some(m) = &self.registry_metrics {
            m.inner.split_pending.set(self.metrics.pending as f64);
        }
        true
    }
    pub fn delay(&mut self, expected: &SplitScatterPendingItem, now: Duration) -> bool {
        if !self.observer.is_current(expected) {
            return false;
        }
        self.retry
            .get_mut(&expected.region_id)
            .expect("recorded retry state")
            .retry_at = now + SPLIT_SCATTER_RETRY_BACKOFF;
        true
    }
    /// Explicit cancellation/shutdown resets the gauge and dispatcher deadline.
    pub fn clear(&mut self) {
        self.observer.clear();
        self.retry.clear();
        self.metrics.pending = 0;
        if let Some(m) = &self.registry_metrics {
            m.inner.split_pending.set(self.metrics.pending as f64);
        }
        self.next_dispatch_at = Duration::ZERO;
    }
    pub fn cleanup_expired(&mut self, now: Duration) -> usize {
        for p in self.observer.snapshot() {
            if !p.expire_at.is_zero() && now >= p.expire_at {
                let attempted = self.retry.get(&p.region_id).is_some_and(|r| r.attempted);
                if self.delete(&p) {
                    if attempted {
                        {
                            self.metrics.expired_attempted += 1;
                            if let Some(m) = &self.registry_metrics {
                                m.inner.expired_attempted.inc_by((1) as f64);
                            }
                        };
                    } else {
                        {
                            self.metrics.expired_unattempted += 1;
                            if let Some(m) = &self.registry_metrics {
                                m.inner.expired_unattempted.inc_by((1) as f64);
                            }
                        };
                    }
                }
            }
        }
        self.observer.len()
    }
    /// Whole batches are rejected when capacity is insufficient, after TTL cleanup.
    pub fn record_batch(
        &mut self,
        source: u64,
        version: u64,
        children: &[u64],
        now: Duration,
        enabled: bool,
    ) {
        if children.is_empty() || !enabled {
            return;
        }
        self.cleanup_expired(now);
        // Preserve Go's per-input count (even repeated child IDs) at the capacity boundary.
        let new_count = usize::from(self.observer.pending(source).is_none())
            + children
                .iter()
                .filter(|&&id| self.observer.pending(id).is_none())
                .count();
        if self.observer.len() + new_count > SPLIT_SCATTER_PENDING_LIMIT {
            {
                self.metrics.dropped += new_count as u64;
                if let Some(m) = &self.registry_metrics {
                    m.inner.dropped.inc_by((new_count as u64) as f64);
                }
            };
            return;
        }
        self.observer.record_batch(
            source,
            version,
            children,
            now + SPLIT_SCATTER_PENDING_TTL,
            true,
        );
        for id in children.iter().copied().chain(std::iter::once(source)) {
            self.retry.insert(id, RetryState::default());
        }
        self.metrics.pending = self.observer.len();
        if let Some(m) = &self.registry_metrics {
            m.inner.split_pending.set(self.metrics.pending as f64);
        }
        self.next_dispatch_at = Duration::ZERO;
    }
    /// Select by deadline, group, ID before applying limit; only selected results
    /// become attempted. Missing results back off without consuming attempts.
    pub fn top_pending(
        &mut self,
        regions: &RegionsInfo,
        operators: &ControllerAdmission,
        now: Duration,
        limit: usize,
    ) -> Vec<SplitScatterPendingItem> {
        if limit == 0 {
            return vec![];
        }
        self.cleanup_expired(now);
        let mut candidates = Vec::new();
        for p in self.observer.snapshot() {
            if now < self.retry[&p.region_id].retry_at {
                continue;
            }
            let result = regions.get_region(p.region_id);
            if let Some(result) = &result {
                let version = split_scatter_region_version(Some(result));
                if version > p.split_version {
                    self.delete(&p);
                    continue;
                }
                if version < p.split_version {
                    continue;
                }
                if operators
                    .get_operator(p.region_id)
                    .is_some_and(|op| op.desc() == INTERNAL_SPLIT_SCATTER_DESC)
                {
                    self.delay(&p, now);
                    continue;
                }
            }
            match observe_split_scatter_result(&p, regions) {
                SplitScatterObservation::MissingResult | SplitScatterObservation::MissingSource => {
                    {
                        self.metrics.missing += 1;
                        if let Some(m) = &self.registry_metrics {
                            m.inc(
                                crate::CheckerMetricKind::SplitScatter,
                                "dispatch-region-missing",
                            );
                        }
                    };
                    self.delay(&p, now);
                }
                SplitScatterObservation::Ready => candidates.push(p),
                _ => {}
            }
        }
        candidates.sort_by(|a, b| {
            a.expire_at
                .cmp(&b.expire_at)
                .then(a.group.cmp(&b.group))
                .then(a.region_id.cmp(&b.region_id))
        });
        candidates.truncate(limit);
        for p in &candidates {
            self.retry.get_mut(&p.region_id).unwrap().attempted = true;
        }
        candidates
    }
    /// Plan sequentially against the actual admission controller. The scatter
    /// planner is supplied by the host (scatter migration follows this task);
    /// labels, replication, store limits and admission use canonical components.
    pub async fn dispatch<C, F>(
        &mut self,
        cluster: &BasicCluster,
        operators: &ControllerAdmission,
        replication: &RegionReplicationContext<'_, C>,
        labels: Option<&RegionLabeler>,
        now: Duration,
        limit: u64,
        mut scatter: F,
    ) -> Vec<(u64, SplitScatterDispatchError)>
    where
        C: SharedConfigProvider,
        F: FnMut(
            &RegionInfo,
            &str,
            &SplitScatterRangeHint,
            &ControllerAdmission,
        ) -> Result<Option<Operator>, SplitScatterDispatchError>,
    {
        let mut errors = vec![];
        if self.cleanup_expired(now) == 0 {
            return errors;
        }
        if limit == 0 {
            {
                self.metrics.disabled += 1;
                if let Some(m) = &self.registry_metrics {
                    m.inc(crate::CheckerMetricKind::SplitScatter, "dispatch-disabled");
                }
            };
            self.clear();
            return errors;
        }
        if now < self.next_dispatch_at {
            return errors;
        }
        let running = operators.operator_count(OpKind::SPLIT_SCATTER);
        if running >= limit {
            {
                self.metrics.schedule_limit += 1;
                if let Some(m) = &self.registry_metrics {
                    m.inc(
                        crate::CheckerMetricKind::SplitScatter,
                        "dispatch-schedule-limit",
                    );
                }
            };
            self.next_dispatch_at = now + SPLIT_SCATTER_RETRY_BACKOFF;
            return errors;
        }
        let pending =
            self.top_pending(&cluster.regions, operators, now, (limit - running) as usize);
        if pending.is_empty() {
            self.next_dispatch_at = now + SPLIT_SCATTER_RETRY_BACKOFF;
            return errors;
        }
        for p in pending {
            // Recheck the canonical state before any planning or admission.
            if !self.is_current(&p) {
                continue;
            }
            match observe_split_scatter_result(&p, &cluster.regions) {
                SplitScatterObservation::StaleResult => {
                    self.delete(&p);
                    continue;
                }
                SplitScatterObservation::MissingResult | SplitScatterObservation::MissingSource => {
                    {
                        self.metrics.missing += 1;
                        if let Some(m) = &self.registry_metrics {
                            m.inc(
                                crate::CheckerMetricKind::SplitScatter,
                                "dispatch-region-missing",
                            );
                        }
                    };
                    self.delay(&p, now);
                    continue;
                }
                SplitScatterObservation::Ready => {}
                _ => continue,
            }
            let region = cluster.regions.get_region(p.region_id).unwrap();
            if labels.is_some_and(|l| l.schedule_disabled(&region, replication.now)) {
                {
                    self.metrics.schedule_disabled += 1;
                    if let Some(m) = &self.registry_metrics {
                        m.inc(
                            crate::CheckerMetricKind::SplitScatter,
                            "dispatch-schedule-disabled",
                        );
                    }
                };
                self.delay(&p, now);
                continue;
            }
            if !is_region_replicated(replication, &region) {
                {
                    self.metrics.not_replicated += 1;
                    if let Some(m) = &self.registry_metrics {
                        m.inc(
                            crate::CheckerMetricKind::SplitScatter,
                            "dispatch-not-fully-replicated",
                        );
                    }
                };
                self.delay(&p, now);
                continue;
            }
            let validate = |id| has_split_scatter_txn_keyspace_bounds(&cluster.regions, id);
            let hint = resolve_split_scatter_range_hint(&region, Some(&validate));
            let group = if hint.scatter_group.is_empty() {
                &p.group
            } else {
                &hint.scatter_group
            };
            let result = match scatter(&region, group, &hint, operators) {
                Ok(Some(op)) => {
                    op.additional_info().set("batch-group", p.group.clone());
                    match operators
                        .add_limited_operators(cluster, replication.conf, &[Arc::new(op)])
                        .await
                    {
                        Ok(_) => {
                            {
                                self.metrics.operator_created += 1;
                                if let Some(m) = &self.registry_metrics {
                                    m.inc(
                                        crate::CheckerMetricKind::SplitScatter,
                                        "dispatch-operator-created",
                                    );
                                }
                            };
                            Ok(())
                        }
                        Err(e) => {
                            if e == AdmissionError::Rejected(CancelReason::ExceedStoreLimit) {
                                {
                                    self.metrics.store_limit += 1;
                                    if let Some(m) = &self.registry_metrics {
                                        m.inc(
                                            crate::CheckerMetricKind::SplitScatter,
                                            "dispatch-store-limit",
                                        );
                                    }
                                };
                            } else {
                                {
                                    self.metrics.add_operator_failed += 1;
                                    if let Some(m) = &self.registry_metrics {
                                        m.inc(
                                            crate::CheckerMetricKind::SplitScatter,
                                            "dispatch-add-operator-failed",
                                        );
                                    }
                                };
                            }
                            Err(SplitScatterDispatchError::Admission(e))
                        }
                    }
                }
                Ok(None) => {
                    {
                        self.metrics.no_operator_needed += 1;
                        if let Some(m) = &self.registry_metrics {
                            m.inc(
                                crate::CheckerMetricKind::SplitScatter,
                                "dispatch-no-operator-needed",
                            );
                        }
                    };
                    Ok(())
                }
                Err(e) => {
                    match &e {
                        SplitScatterDispatchError::RetryLater(
                            SplitScatterRetryReason::HotRegion,
                        ) => {
                            self.metrics.hot += 1;
                            if let Some(m) = &self.registry_metrics {
                                m.inc(
                                    crate::CheckerMetricKind::SplitScatter,
                                    "dispatch-hot-region",
                                );
                            }
                        }
                        SplitScatterDispatchError::RetryLater(
                            SplitScatterRetryReason::BalancedReadCpu,
                        ) => {
                            self.metrics.balanced_read_cpu += 1;
                            if let Some(m) = &self.registry_metrics {
                                m.inc(
                                    crate::CheckerMetricKind::SplitScatter,
                                    "dispatch-balanced-read-cpu",
                                );
                            }
                        }
                        _ => {
                            self.metrics.scatter_failed += 1;
                            if let Some(m) = &self.registry_metrics {
                                m.inc(
                                    crate::CheckerMetricKind::SplitScatter,
                                    "dispatch-scatter-failed",
                                );
                            }
                        }
                    }
                    Err(e)
                }
            };
            match result {
                Ok(()) => {
                    self.delete(&p);
                }
                Err(error) => {
                    self.delay(&p, now);
                    errors.push((p.region_id, error));
                }
            }
        }
        errors
    }
}
