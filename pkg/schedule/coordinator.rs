// Copyright 2026 AsterSQL.
//! Component ownership and short-lived views over one canonical scheduling graph.
use crate::PrepareChecker;
use astersql_pd_cache::TTLUint64;
use astersql_pd_core::BasicCluster;
use astersql_pd_schedule_checker::{CheckerDispatcher, MergeChecker, RuleFaultChecker};
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_diagnostic::Manager;
use astersql_pd_schedule_filter::{RegionFilter, RegionReplicationContext};
use astersql_pd_schedule_hbstream::routing::RoutingHandle;
use astersql_pd_schedule_operator::{ControllerAdmission, ControllerRecords};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_scatter::{ScatterBatch, ScatterBatchMetrics, ScatterCounts};
use astersql_pd_schedule_schedulers::{DiagnosticRecorder, FactoryContext, SchedulerController};
use astersql_pd_schedule_splitter::{SplitRetry, SplitSubmissions};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub type PrepareRegionCount = Box<dyn Fn() -> Result<i64, String> + Send + Sync>;
/// Host capabilities. Cache shutdown, heartbeat routing and clocks remain host-owned.
pub struct CoordinatorInputs<C> {
    pub cluster: Arc<BasicCluster>,
    pub config: Arc<C>,
    pub heartbeat: RoutingHandle,
    pub pending: Arc<TTLUint64<()>>,
    pub factory: FactoryContext,
    pub scheduler_configuration: serde_json::Value,
    pub scatter_metrics: ScatterBatchMetrics,
    pub start: Duration,
    pub now_unix_nanos: i128,
    pub split_merge_interval_nanos: i64,
    pub total_regions: PrepareRegionCount,
    pub unix_seconds: Arc<dyn Fn() -> u64 + Send + Sync>,
}
/// One owner for canonical controllers; the host serializes mutable facade access.
/// Records must be published outside the checker lock because success reenters it.
pub struct Coordinator<C> {
    cluster: Arc<BasicCluster>,
    config: Arc<C>,
    heartbeat: RoutingHandle,
    prepare: PrepareChecker<PrepareRegionCount>,
    checkers: Arc<Mutex<CheckerDispatcher>>,
    operators: Arc<ControllerAdmission>,
    records: ControllerRecords,
    schedulers: SchedulerController,
    schedulers_initialized: bool,
    recorders: BTreeMap<String, DiagnosticRecorder>,
    scatter_counts: ScatterCounts,
    scatter_metrics: ScatterBatchMetrics,
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Coordinator<C> {
    pub fn new(input: CoordinatorInputs<C>) -> Self {
        let checkers = Arc::new(Mutex::new(CheckerDispatcher::new(
            input.pending,
            input.now_unix_nanos,
            input.split_merge_interval_nanos,
        )));
        let config = input.config.clone();
        let seconds = input.unix_seconds;
        let callback = CheckerDispatcher::success_callback(Arc::downgrade(&checkers), move || {
            (config.get_affinity_schedule_limit(), seconds())
        });
        let operators = Arc::new(ControllerAdmission::new(
            input.config.get_scheduler_max_waiting_operator(),
        ));
        Self {
            schedulers_initialized: false,
            schedulers: SchedulerController::new(
                Some(input.cluster.clone()),
                input.factory,
                input.scheduler_configuration,
            ),
            prepare: PrepareChecker::new(input.start, input.total_regions),
            records: ControllerRecords::default().with_success_callbacks(vec![Some(callback)]),
            checkers,
            operators,
            cluster: input.cluster,
            config: input.config,
            heartbeat: input.heartbeat,
            recorders: BTreeMap::new(),
            scatter_counts: ScatterCounts::default(),
            scatter_metrics: input.scatter_metrics,
        }
    }
    /// True after configuration cleanup, including a nonfatal final persist error.
    pub fn are_schedulers_initialized(&self) -> bool {
        self.schedulers_initialized
    }
    pub async fn init_schedulers(
        &mut self,
        need_run: bool,
        cancellation: &CancellationToken,
    ) -> Result<crate::SchedulerRestoreReport, crate::SchedulerRestoreError> {
        let report =
            crate::restore_schedulers(&mut self.schedulers, need_run, cancellation).await?;
        self.schedulers_initialized = true;
        Ok(report)
    }
    pub fn get_cluster(&self) -> &Arc<BasicCluster> {
        &self.cluster
    }
    pub fn get_heartbeat_streams(&self) -> &RoutingHandle {
        &self.heartbeat
    }
    pub fn get_checker_controller(&self) -> &Arc<Mutex<CheckerDispatcher>> {
        &self.checkers
    }
    pub fn get_rule_checker(&self) -> RuleCheckerGuard<'_> {
        RuleCheckerGuard(self.checkers.lock().unwrap())
    }
    pub fn get_merge_checker(&self) -> MergeCheckerGuard<'_> {
        MergeCheckerGuard(self.checkers.lock().unwrap())
    }
    pub fn get_config(&self) -> &Arc<C> {
        &self.config
    }
    pub fn get_operator_controller(&self) -> &ControllerAdmission {
        &self.operators
    }
    pub fn operator_handle(&self) -> Arc<ControllerAdmission> {
        self.operators.clone()
    }
    pub fn get_operator_records(&self) -> &ControllerRecords {
        &self.records
    }
    pub fn get_schedulers_controller(&self) -> &SchedulerController {
        &self.schedulers
    }
    pub fn get_schedulers_controller_mut(&mut self) -> &mut SchedulerController {
        &mut self.schedulers
    }
    pub fn diagnostic_recorders_mut(&mut self) -> &mut BTreeMap<String, DiagnosticRecorder> {
        &mut self.recorders
    }
    pub fn get_diagnostic_manager(&self) -> Manager<'_, C> {
        Manager::new(&self.schedulers, &*self.config, &self.recorders)
    }
    pub fn get_prepare_checker(&self) -> &PrepareChecker<PrepareRegionCount> {
        &self.prepare
    }
    pub fn should_run(&self, now: Duration) -> bool {
        self.prepare.should_run(&self.cluster, now)
    }
    pub fn is_pending_region(&self, id: u64) -> bool {
        self.checkers.lock().unwrap().is_pending_region(id)
    }
    pub fn record_op_step_with_ttl(&self, id: u64, now: i128) {
        self.checkers
            .lock()
            .unwrap()
            .record_region_promote_to_non_witness(
                id,
                now,
                self.config.get_switch_witness_interval().as_nanos() as i128,
            );
    }
    /// Planning state is retained here; each call borrows current rules and hot inputs.
    pub fn get_region_scatterer<'a>(
        &'a mut self,
        rules: &'a mut RuleManager,
        hot: &'a BTreeSet<u64>,
        affinity: Option<&'a dyn RegionFilter>,
        now: i128,
        supports_conf_change_v2: bool,
    ) -> RegionScatterer<'a, C> {
        RegionScatterer {
            batch: ScatterBatch {
                cluster: &self.cluster,
                config: &*self.config,
                rules,
                controller: &self.operators,
                counts: &mut self.scatter_counts,
                affinity,
                hot_regions: hot,
                now_unix_nanos: now,
                supports_conf_change_v2,
                metrics: &self.scatter_metrics,
            },
            checkers: &self.checkers,
        }
    }
    pub fn get_region_splitter(&self) -> RegionSplitter<'_> {
        RegionSplitter {
            cluster: &self.cluster,
            controller: &self.operators,
            checkers: &self.checkers,
        }
    }
}
/// Canonical scatter algorithm plus the coordinator's checker pending callback.
pub struct RegionScatterer<'a, C: CheckerConfigProvider> {
    pub batch: ScatterBatch<'a, C>,
    checkers: &'a Arc<Mutex<CheckerDispatcher>>,
}
impl<C: CheckerConfigProvider> RegionScatterer<'_, C> {
    pub async fn scatter<F: FnMut() -> Result<u64, astersql_pd_schedule_operator::BuilderError>>(
        &mut self,
        request: astersql_pd_schedule_scatter::ScatterBatchRequest,
        group: &str,
        retries: i32,
        skip_store_limit: bool,
        cancellation: &CancellationToken,
        allocate: F,
    ) -> Result<
        astersql_pd_schedule_scatter::ScatterBatchOutcome,
        astersql_pd_schedule_scatter::ScatterBatchError,
    > {
        let outcome = self
            .batch
            .scatter(
                request,
                group,
                retries,
                skip_store_limit,
                cancellation,
                allocate,
            )
            .await?;
        self.checkers.lock().unwrap().add_pending_processed_regions(
            false,
            &outcome.suspect.iter().copied().collect::<Vec<_>>(),
        );
        Ok(outcome)
    }
}
/// Split retry planning shares admission and routes replication failures to checkers.
pub struct RegionSplitter<'a> {
    pub cluster: &'a BasicCluster,
    pub controller: &'a ControllerAdmission,
    checkers: &'a Arc<Mutex<CheckerDispatcher>>,
}
impl RegionSplitter<'_> {
    pub fn start_attempt<C: CheckerConfigProvider>(
        &self,
        retry: &mut SplitRetry,
        now_ms: u64,
        replication: &RegionReplicationContext<'_, C>,
    ) -> Option<SplitSubmissions> {
        retry.start_attempt(
            now_ms,
            self.cluster,
            self.controller,
            replication,
            |limit, id| {
                self.checkers
                    .lock()
                    .unwrap()
                    .add_pending_processed_regions(limit, &[id]);
            },
        )
    }
}

/// Locked view of the rule checker used by the dispatcher. Drop before reentering.
pub struct RuleCheckerGuard<'a>(MutexGuard<'a, CheckerDispatcher>);
impl Deref for RuleCheckerGuard<'_> {
    type Target = RuleFaultChecker;
    fn deref(&self) -> &Self::Target {
        self.0.rule_checker()
    }
}
/// Locked view of the merge checker used by the dispatcher. Drop before reentering.
pub struct MergeCheckerGuard<'a>(MutexGuard<'a, CheckerDispatcher>);
impl Deref for MergeCheckerGuard<'_> {
    type Target = MergeChecker;
    fn deref(&self) -> &Self::Target {
        self.0.merge_checker()
    }
}

#[path = "coordinator_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::*;
