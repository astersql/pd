// Copyright 2026 AsterSQL.
//! Serial admin batch scatter over canonical region snapshots and admission state.
use crate::{
    ScatterCounts, ScatterOperatorError, ScatterTargetError, ScatterTargetSelector,
    create_scatter_operator,
};
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{RegionFilter, RegionReplicationContext, is_region_replicated};
use astersql_pd_schedule_operator::{
    AdmissionError, BuilderContext, BuilderError, ControllerAdmission, Operator,
};
use astersql_pd_schedule_placement::RuleManager;
use prometheus::{Counter, CounterVec, Opts, Registry};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// IDs are deduplicated after lookup; range scans use the canonical region index.
pub enum ScatterBatchRequest {
    Ids(Vec<u64>),
    Range { start: Vec<u8>, end: Vec<u8> },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScatterBatchError {
    EmptyRegion,
    RegionNotFound(u64),
}
impl std::fmt::Display for ScatterBatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scatter batch error: {self:?}")
    }
}
impl std::error::Error for ScatterBatchError {}
/// Per-region errors retain typed builder, selector and admission causes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScatterBatchFailure {
    MissingRegion,
    NotReplicated,
    ExistingOperator,
    NoLeader,
    Hot,
    Target(ScatterTargetError),
    Operator(ScatterOperatorError),
    Admission(AdmissionError),
    Cancelled,
}
impl std::fmt::Display for ScatterBatchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scatter region error: {self:?}")
    }
}
impl std::error::Error for ScatterBatchFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Target(e) => Some(e),
            Self::Operator(e) => Some(e),
            Self::Admission(e) => Some(e),
            _ => None,
        }
    }
}
#[derive(Debug, Default)]
pub struct ScatterBatchOutcome {
    /// Successful Scatter calls, including no-ops and controller rejections (Go opsCount).
    pub processed: usize,
    pub failures: BTreeMap<u64, ScatterBatchFailure>,
    /// Regions still eligible for retry; excludes missing IDs and admission failures.
    pub pending: BTreeSet<u64>,
    /// Not-replicated regions to pass to the host's addSuspectRegions(false, ids...).
    pub suspect: BTreeSet<u64>,
    pub cancelled: bool,
}
/// Existing Go metric name/labels, cached bounded vocabulary; no global registry.
#[derive(Clone)]
pub struct ScatterBatchMetrics {
    counters: BTreeMap<(&'static str, &'static str), Counter>,
}
impl ScatterBatchMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let vector = CounterVec::new(
            Opts::new(
                "pd_schedule_scatter_operators_count",
                "Counter of region scatter operators.",
            ),
            &["type", "event"],
        )?;
        let counters = [
            ("skip", "empty-region"),
            ("skip", "no-region"),
            ("skip", "not-replicated"),
            ("skip", "no-leader"),
            ("skip", "running"),
            ("fail", "other-existed"),
            ("skip", "affinity"),
            ("skip", "hot"),
            ("fail", ""),
            ("success", ""),
            ("unnecessary", ""),
        ]
        .into_iter()
        .map(|key| (key, vector.with_label_values(&[key.0, key.1])))
        .collect();
        registry.register(Box::new(vector))?;
        Ok(Self { counters })
    }
    fn inc(&self, kind: &'static str, event: &'static str) {
        self.counters[&(kind, event)].inc();
    }
    pub fn value(&self, kind: &'static str, event: &'static str) -> f64 {
        self.counters.get(&(kind, event)).map_or(0.0, Counter::get)
    }
}
/// Host serializes planning/accounting and supplies canonical affinity/hot snapshots.
/// Allocation is the sole injected external I/O boundary. No private runtime or task.
pub struct ScatterBatch<'a, C: SharedConfigProvider> {
    pub cluster: &'a BasicCluster,
    pub config: &'a C,
    pub rules: &'a mut RuleManager,
    pub controller: &'a ControllerAdmission,
    pub counts: &'a mut ScatterCounts,
    /// The canonical AffinityFilter; None only when affinity is disabled.
    pub affinity: Option<&'a dyn RegionFilter>,
    pub hot_regions: &'a BTreeSet<u64>,
    pub now_unix_nanos: i128,
    pub supports_conf_change_v2: bool,
    pub metrics: &'a ScatterBatchMetrics,
}
impl<C: SharedConfigProvider> ScatterBatch<'_, C> {
    /// Plan one real scatter action without admission, for single-region admin dispatch.
    pub async fn scatter_one<F: FnMut() -> Result<u64, BuilderError>>(
        &mut self,
        region: &Arc<RegionInfo>,
        group: &str,
        skip: bool,
        suspect: &mut BTreeSet<u64>,
        allocate: &mut F,
    ) -> Result<Option<Operator>, ScatterBatchFailure> {
        let mut stores = self.cluster.stores.get_stores();
        stores.sort_by_key(|s| s.id());
        let replicated = is_region_replicated(
            &RegionReplicationContext {
                conf: self.config,
                stores: &stores,
                manager: RefCell::new(&mut *self.rules),
                now: self.now_unix_nanos,
            },
            region,
        );
        if !replicated {
            suspect.insert(region.meta().id);
            self.metrics.inc("skip", "not-replicated");
            return Err(ScatterBatchFailure::NotReplicated);
        }
        if let Some(op) = self.controller.get_operator(region.meta().id) {
            if op.priority_level().as_i32() >= PriorityLevel::HIGH.as_i32() {
                if op.desc() == "scatter-region"
                    && op.additional_info().get("group").as_deref() == Some(group)
                    && op.region_epoch() == region.meta().epoch.as_ref()
                {
                    self.metrics.inc("skip", "running");
                    return Ok(None);
                }
                self.metrics.inc("fail", "other-existed");
                return Err(ScatterBatchFailure::ExistingOperator);
            }
        }
        if region.leader().is_none() {
            self.metrics.inc("skip", "no-leader");
            return Err(ScatterBatchFailure::NoLeader);
        }
        if self.affinity.is_some_and(|f| !f.select(region).is_ok()) {
            self.metrics.inc("skip", "affinity");
            return Ok(None);
        }
        if self.hot_regions.contains(&region.meta().id) {
            self.metrics.inc("skip", "hot");
            return Err(ScatterBatchFailure::Hot);
        }
        let rules = self.rules.get_rules_for_apply_region(region);
        let target = ScatterTargetSelector {
            config: self.config,
            stores: &stores,
            rules: &rules,
            counts: self.counts,
            now_unix_nanos: self.now_unix_nanos,
        }
        .select(region, group, &BTreeSet::new())
        .await
        .map_err(ScatterBatchFailure::Target)?;
        let result = create_scatter_operator(
            BuilderContext {
                stores: &self.cluster.stores,
                config: self.config,
                rules: &rules,
                now_unix_nanos: self.now_unix_nanos,
            },
            region,
            &target,
            group,
            skip,
            self.supports_conf_change_v2,
            self.counts,
            allocate,
        )
        .await;
        match &result {
            Ok(Some(_)) => self.metrics.inc("success", ""),
            Ok(None) => self.metrics.inc("unnecessary", ""),
            Err(_) => self.metrics.inc("fail", ""),
        }
        result.map_err(ScatterBatchFailure::Operator)
    }
    /// Initial attempt plus min(retry_limit,30) retries. A negative limit performs
    /// no attempts. Retains Go's final failed-round sleep, capped at 60 seconds.
    /// Cancellation stops before the next region or during backoff, preserving
    /// completed results and the latest typed errors for unresolved regions.
    pub async fn scatter<F: FnMut() -> Result<u64, BuilderError>>(
        &mut self,
        request: ScatterBatchRequest,
        group: &str,
        retry_limit: i32,
        skip_store_limit: bool,
        cancellation: &CancellationToken,
        mut allocate: F,
    ) -> Result<ScatterBatchOutcome, ScatterBatchError> {
        let mut outcome = ScatterBatchOutcome::default();
        let mut skip = skip_store_limit;
        let snapshots = match request {
            ScatterBatchRequest::Ids(ids) => {
                if ids.is_empty() {
                    self.metrics.inc("skip", "empty-region");
                    return Err(ScatterBatchError::EmptyRegion);
                }
                if ids.len() == 1 && self.cluster.regions.get_region(ids[0]).is_none() {
                    self.metrics.inc("skip", "no-region");
                    return Err(ScatterBatchError::RegionNotFound(ids[0]));
                }
                ids.into_iter()
                    .filter_map(|id| match self.cluster.regions.get_region(id) {
                        Some(r) => Some(r),
                        None => {
                            self.metrics.inc("skip", "no-region");
                            outcome
                                .failures
                                .insert(id, ScatterBatchFailure::MissingRegion);
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            }
            ScatterBatchRequest::Range { start, end } => {
                skip = false;
                self.cluster.regions.scan_regions(&start, &end, -1)
            }
        };
        let mut regions: BTreeMap<_, _> = snapshots.into_iter().map(|r| (r.meta().id, r)).collect();
        if regions.is_empty() {
            self.metrics.inc("skip", "empty-region");
            return Err(ScatterBatchError::EmptyRegion);
        }
        for round in 0..=retry_limit.min(30) {
            let ids: Vec<_> = regions.keys().copied().collect();
            for id in ids {
                if cancellation.is_cancelled() {
                    outcome.cancelled = true;
                    break;
                }
                let region = &regions[&id];
                match self
                    .scatter_one(region, group, skip, &mut outcome.suspect, &mut allocate)
                    .await
                {
                    Err(e) => {
                        outcome.failures.insert(id, e);
                    }
                    Ok(op) => {
                        regions.remove(&id);
                        outcome.processed += 1;
                        if let Some(op) = op {
                            if let Err(error) = self
                                .controller
                                .add_limited_operators(self.cluster, self.config, &[Arc::new(op)])
                                .await
                            {
                                outcome
                                    .failures
                                    .insert(id, ScatterBatchFailure::Admission(error));
                                continue;
                            }
                        }
                        outcome.failures.remove(&id);
                    }
                }
            }
            if regions.is_empty() || outcome.cancelled {
                break;
            }
            let delay = Duration::from_millis((100_u64 << round).min(60_000));
            tokio::select! { biased; _=cancellation.cancelled()=>{outcome.cancelled=true;break;}, _=tokio::time::sleep(delay)=>{} }
        }
        outcome.pending = regions.keys().copied().collect();
        if outcome.cancelled {
            for &id in &outcome.pending {
                outcome
                    .failures
                    .entry(id)
                    .or_insert(ScatterBatchFailure::Cancelled);
            }
        }
        Ok(outcome)
    }
}
