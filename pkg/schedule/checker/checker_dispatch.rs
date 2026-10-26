// Copyright 2026 AsterSQL.
//! Ordered checker dispatch over host-owned state and canonical pending caches.
use crate::*;
use astersql_pd_cache::{Cache, TTLUint64};
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_schedule_affinity::manager::Manager;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_operator::{
    AdmissionError, BuilderContext, BuilderError, ControllerAdmission, OpKind, Operator, SUCCESS,
    SuccessCallback, exceed_store_limit,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_placement::region_rule_cache::{
    validate_fit, validate_region, validate_stores,
};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::{Arc, Mutex, Weak},
};

pub const DEFAULT_PENDING_REGION_CACHE_SIZE: usize = 100_000;
/// Host-owned canonical cluster and persistent affinity dependencies.
pub struct CheckerDispatchContext<'a, C: CheckerConfigProvider> {
    pub affinity: AffinityContext<'a, C>,
    pub cluster: &'a BasicCluster,
    pub operators: &'a ControllerAdmission,
    pub hot_regions: &'a BTreeSet<u64>,
}
/// Typed diagnostics do not suppress later checker stages.
#[derive(Debug, PartialEq)]
pub enum DispatchError {
    Builder(BuilderError),
    Missing(RuleMissingError),
    Rule(RuleFaultError),
}
#[derive(Default)]
pub struct CheckerDispatchOutcome {
    pub operators: Vec<Operator>,
    pub errors: Vec<DispatchError>,
    pub rule_cache_hit: bool,
    pub schedule_denied: bool,
}
/// Owns checker state only. The host creates the shared three-minute TTL cache
/// using its runtime/cancellation token and joins cache shutdown itself.
pub struct CheckerDispatcher {
    metrics: Option<CheckerMetrics>,
    joint: JointStateChecker,
    split: SplitChecker,
    learner: LearnerChecker,
    replica_fault: ReplicaFaultChecker,
    replica_shape: ReplicaShapeChecker,
    replica_pause: PauseController,
    rule_fault: RuleFaultChecker,
    rule_location: RuleLocationChecker,
    rule_pause: PauseController,
    affinity: AffinityChecker,
    merge: MergeChecker,
    pub priority: PriorityInspector,
    pending: Arc<TTLUint64<()>>,
    rule_pending: Cache<()>,
}
impl CheckerDispatcher {
    pub fn new(pending: Arc<TTLUint64<()>>, now: i128, split_merge_interval: i64) -> Self {
        Self {
            metrics: None,
            joint: JointStateChecker::default(),
            split: SplitChecker::default(),
            learner: LearnerChecker::default(),
            replica_fault: ReplicaFaultChecker::default(),
            replica_shape: ReplicaShapeChecker::default(),
            replica_pause: PauseController::default(),
            rule_fault: RuleFaultChecker::default(),
            rule_location: RuleLocationChecker::default(),
            rule_pause: PauseController::default(),
            affinity: AffinityChecker::new(now),
            merge: MergeChecker::new(now, split_merge_interval),
            priority: PriorityInspector::new(),
            pending,
            rule_pending: Cache::new_default(100_000),
        }
    }
    /// Attach one host Registry before dispatch starts; no global registration.
    pub fn set_metrics(&mut self, metrics: CheckerMetrics) {
        self.metrics = Some(metrics);
    }
    /// Canonical rule state used by dispatch and heartbeat completion callbacks.
    pub fn rule_checker(&self) -> &RuleFaultChecker {
        &self.rule_fault
    }
    /// Canonical merge state used by dispatch.
    pub fn merge_checker(&self) -> &MergeChecker {
        &self.merge
    }
    /// Stable Go checker names; unknown names are rejected without modifying state.
    pub fn pause_controller(&self, name: &str) -> Option<&PauseController> {
        Some(match name {
            "joint-state" => &self.joint.pause,
            "split" => &self.split.pause,
            "learner" => &self.learner.pause,
            "replica" => &self.replica_pause,
            "rule" => &self.rule_pause,
            "affinity" => &self.affinity.pause,
            "merge" => &self.merge.pause,
            _ => return None,
        })
    }
    pub fn pending_processed_regions(&self) -> Vec<u64> {
        self.pending.get_all_id()
    }
    /// Preserve the strict greater-than check, including its one-entry overshoot.
    pub fn add_pending_processed_regions(&self, check_len: bool, ids: &[u64]) {
        for &id in ids {
            if check_len && self.pending.len() > DEFAULT_PENDING_REGION_CACHE_SIZE {
                return;
            }
            self.pending.put(id, ());
        }
    }
    pub fn remove_pending_processed_region(&self, id: u64) {
        self.pending.remove(&id);
    }
    pub fn clear_pending_processed_regions(&self) {
        self.pending.clear();
    }
    pub fn is_pending_region(&self, id: u64) -> bool {
        self.rule_pending.get(id).is_some()
    }
    /// First nonempty stage wins; errors remain observable while later checks run.
    pub async fn check_region<C, F>(
        &mut self,
        ctx: &CheckerDispatchContext<'_, C>,
        rules: &mut RuleManager,
        affinity: &mut Manager,
        region: &Arc<RegionInfo>,
        allocate: &mut F,
    ) -> CheckerDispatchOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let metrics = self.metrics.clone();
        let mut out = CheckerDispatchOutcome::default();
        let rep = &ctx.affinity.rules.replica;
        let conf = rep.config;
        let seconds = rep.now_unix_nanos.div_euclid(1_000_000_000) as i64;
        let apply = rules.get_rules_for_apply_region(region);
        let builder = || BuilderContext {
            stores: rep.stores,
            config: conf,
            rules: &apply,
            now_unix_nanos: rep.now_unix_nanos,
        };
        {
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Joint));
            if let Some(m) = &metrics {
                m.inc(CheckerMetricKind::Joint, "check");
                if self.joint.pause.is_paused_at(seconds) {
                    m.inc(CheckerMetricKind::Joint, "paused");
                }
            }
            let result = self.joint.check(builder(), region).await;
            if let Some(m) = &metrics {
                if !result.errors.is_empty() {
                    m.inc(CheckerMetricKind::Joint, "create-operator-fail");
                }
                if let Some(op) = &result.operator {
                    m.inc(CheckerMetricKind::Joint, "new-operator");
                    if op.len() > 1 {
                        m.inc(CheckerMetricKind::Joint, "transfer-leader");
                    }
                }
            }
            out.errors
                .extend(result.errors.into_iter().map(DispatchError::Builder));
            if let Some(op) = result.operator {
                out.operators.push(op);
                return out;
            }
        }
        {
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Split));
            if let Some(m) = &metrics {
                m.inc(CheckerMetricKind::Split, "check");
                if self.split.pause.is_paused_at(seconds) {
                    m.inc(CheckerMetricKind::Split, "paused");
                }
            }
            match self
                .split
                .check(conf, rules, ctx.affinity.labels, region, seconds)
            {
                Ok(Some(op)) => {
                    out.operators.push(op);
                    return out;
                }
                Err(e) => out.errors.push(DispatchError::Builder(e)),
                _ => {}
            }
        }
        if conf.is_placement_rules_enabled() {
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Rule));
            out.rule_cache_hit = conf.is_placement_rules_cache_enabled()
                && rules.is_region_fit_cached(&rep.stores.get_stores(), region, rep.now_unix_nanos);
            if out.rule_cache_hit {
                if let Some(m) = &metrics {
                    m.inc(CheckerMetricKind::Rule, "get-cache");
                }
            }
            if !out.rule_cache_hit {
                let fit = self.priority.inspect(
                    conf,
                    rules,
                    &rep.stores.get_stores(),
                    region,
                    rep.now_unix_nanos,
                );
                if ctx.operators.operator_count(OpKind::REPLICA) < conf.get_replica_schedule_limit()
                {
                    if let Some(m) = &metrics {
                        if self.rule_pause.is_paused_at(seconds) {
                            m.inc(CheckerMetricKind::Rule, "paused");
                        } else if region.leader().is_none() {
                            m.inc(CheckerMetricKind::Rule, "region-no-leader");
                        } else if fit.is_some() {
                            m.inc(CheckerMetricKind::Rule, "check");
                        }
                    }
                    if !self.rule_pause.is_paused_at(seconds) && region.leader().is_some() {
                        if let Some(fit) = fit {
                            rules.invalid_cache(region.meta().id);
                            self.rule_fault.refresh(rep.stores, rep.now_unix_nanos);
                            if !fit.rule_fits.is_empty() {
                                match self
                                    .rule_location
                                    .fix_orphan_peers(&ctx.affinity.rules, region, &fit, allocate)
                                    .await
                                {
                                    Ok(Some(op)) => {
                                        self.rule_pending.remove(region.meta().id);
                                        out.operators.push(op);
                                        return out;
                                    }
                                    Err(e) => out.errors.push(DispatchError::Builder(e)),
                                    _ => {}
                                }
                                for rf in &fit.rule_fits {
                                    if (rf.peers.len() as i64) < rf.rule.count {
                                        let mut result = RuleMissingOutcome::default();
                                        RuleMissingChecker::default()
                                            .add(
                                                &ctx.affinity.rules,
                                                region,
                                                &fit,
                                                rf,
                                                true,
                                                allocate,
                                                &mut result,
                                            )
                                            .await;
                                        self.apply_rule_pending(
                                            region.meta().id,
                                            result.pending,
                                            result.pending_processed,
                                        );
                                        out.errors.extend(
                                            result.errors.into_iter().map(DispatchError::Missing),
                                        );
                                        if let Some(op) = result.operator {
                                            self.rule_pending.remove(region.meta().id);
                                            out.operators.push(op);
                                            return out;
                                        }
                                        continue;
                                    }
                                    let mut result = RuleFaultOutcome::default();
                                    match self
                                        .rule_fault
                                        .fix_rule(
                                            &ctx.affinity.rules,
                                            region,
                                            &fit,
                                            rf,
                                            allocate,
                                            &mut result,
                                        )
                                        .await
                                    {
                                        Ok(Some(op)) => {
                                            self.rule_pending.remove(region.meta().id);
                                            out.operators.push(op);
                                            return out;
                                        }
                                        Err(e) => {
                                            self.apply_rule_pending(
                                                region.meta().id,
                                                result.pending,
                                                result.pending_processed,
                                            );
                                            out.errors.push(DispatchError::Rule(e));
                                            continue;
                                        }
                                        Ok(None) => {}
                                    }
                                    let result = self
                                        .rule_location
                                        .fix_better_location(
                                            &ctx.affinity.rules,
                                            region,
                                            &fit,
                                            rf,
                                            allocate,
                                        )
                                        .await;
                                    self.apply_rule_pending(
                                        region.meta().id,
                                        result.pending,
                                        result.pending_processed,
                                    );
                                    out.errors.extend(
                                        result.errors.into_iter().map(DispatchError::Missing),
                                    );
                                    if let Some(op) = result.operator {
                                        self.rule_pending.remove(region.meta().id);
                                        out.operators.push(op);
                                        return out;
                                    }
                                }
                                if conf.is_placement_rules_cache_enabled()
                                    && validate_fit(&fit)
                                    && validate_region(region)
                                    && validate_stores(fit.region_stores(), rep.now_unix_nanos)
                                {
                                    rules.set_region_fit_cache(
                                        &rep.stores.get_stores(),
                                        region,
                                        fit,
                                        rep.now_unix_nanos,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    self.pending.put(region.meta().id, ());
                }
            }
        } else {
            {
                let _timer = metrics
                    .as_ref()
                    .map(|m| m.checker_metrics(CheckerMetricKind::Learner));
                if self.learner.pause.is_paused_at(seconds) {
                    if let Some(m) = &metrics {
                        m.inc(CheckerMetricKind::Learner, "paused");
                    }
                }
                let result = self
                    .learner
                    .check(builder(), region, rep.supports_conf_change_v2)
                    .await;
                out.errors
                    .extend(result.errors.into_iter().map(DispatchError::Builder));
                if let Some(op) = result.operator {
                    out.operators.push(op);
                    return out;
                }
            }
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Replica));
            if let Some(m) = &metrics {
                m.inc(CheckerMetricKind::Replica, "check");
                if self.replica_pause.is_paused_at(seconds) {
                    m.inc(CheckerMetricKind::Replica, "paused");
                }
            }
            if !self.replica_pause.is_paused_at(seconds) {
                let context = || ReplicaFaultContext {
                    config: conf,
                    stores: rep.stores,
                    rules: &apply,
                    now_unix_nanos: rep.now_unix_nanos,
                    supports_conf_change_v2: rep.supports_conf_change_v2,
                };
                let mut result = self
                    .replica_fault
                    .check(context(), region, &mut *allocate)
                    .await;
                if result.pending {
                    self.pending.put(region.meta().id, ());
                }
                out.errors
                    .extend(result.errors.into_iter().map(DispatchError::Builder));
                if result.operator.is_none() {
                    result = self
                        .replica_shape
                        .check(context(), region, &mut *allocate)
                        .await;
                    if result.pending {
                        self.pending.put(region.meta().id, ());
                    }
                    out.errors
                        .extend(result.errors.into_iter().map(DispatchError::Builder));
                }
                if let Some(op) = result.operator {
                    if let Some(m) = &metrics {
                        m.inc(CheckerMetricKind::Replica, "new-operator");
                    }
                    if ctx.operators.operator_count(OpKind::REPLICA)
                        < conf.get_replica_schedule_limit()
                    {
                        out.operators.push(op);
                        return out;
                    }
                    self.pending.put(region.meta().id, ());
                }
            }
        }
        if ctx
            .affinity
            .labels
            .schedule_disabled(region, rep.now_unix_nanos)
        {
            out.schedule_denied = true;
            return out;
        }
        let (prev, next) = ctx.cluster.regions.get_adjacent_regions(region);
        {
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Affinity));
            if ctx.operators.operator_count(OpKind::AFFINITY) < conf.get_affinity_schedule_limit() {
                if let Some(m) = &metrics {
                    m.inc(CheckerMetricKind::Affinity, "check");
                    if self.affinity.pause.is_paused_at(seconds) {
                        m.inc(CheckerMetricKind::Affinity, "paused");
                    }
                }
                match self
                    .affinity
                    .check_with_neighbors(
                        &ctx.affinity,
                        rules,
                        affinity,
                        region,
                        prev.as_ref(),
                        next.as_ref(),
                        allocate,
                    )
                    .await
                {
                    Ok(ops) if !ops.is_empty() => {
                        out.operators = ops;
                        return out;
                    }
                    Err(e) => out.errors.push(DispatchError::Builder(e)),
                    _ => {}
                }
            }
        }
        {
            let _timer = metrics
                .as_ref()
                .map(|m| m.checker_metrics(CheckerMetricKind::Merge));
            if ctx.operators.operator_count(OpKind::MERGE) < conf.get_merge_schedule_limit() {
                if let Some(m) = &metrics {
                    m.inc(CheckerMetricKind::Merge, "check");
                    if self.merge.pause.is_paused_at(seconds) {
                        m.inc(CheckerMetricKind::Merge, "paused");
                    }
                }
                let snapshots = rep.stores.get_stores();
                let replication = RegionReplicationContext {
                    conf,
                    stores: &snapshots,
                    manager: RefCell::new(rules),
                    now: rep.now_unix_nanos,
                };
                match self
                    .merge
                    .check(
                        MergeCheckContext {
                            replication: &replication,
                            stores: rep.stores,
                            labels: Some(ctx.affinity.labels),
                            hot_regions: ctx.hot_regions,
                            supports_conf_change_v2: rep.supports_conf_change_v2,
                        },
                        region,
                        prev.as_deref(),
                        next.as_deref(),
                        allocate,
                    )
                    .await
                {
                    Ok(Some(ops)) => {
                        if let Some(m) = &metrics {
                            m.inc(CheckerMetricKind::Merge, "new-operator");
                        }
                        out.operators.extend(ops)
                    }
                    Err(e) => out.errors.push(DispatchError::Builder(e)),
                    _ => {}
                }
            }
        }
        out
    }
    fn apply_rule_pending(&self, id: u64, pending: bool, processed: bool) {
        if processed {
            self.pending.put(id, ());
            self.rule_pending.remove(id);
        }
        // pending is the final filter decision. Both flags can be set after
        // a temporary add rejection followed by a permanent swap-fit rejection.
        if pending {
            self.rule_pending.put(id, ());
        }
    }
    /// Host registers this hook with ControllerRecords. Canonical records ensure
    /// one delivery per successful operator identity, including merge partners.
    pub fn success_callback(
        owner: Weak<Mutex<Self>>,
        current_limit_and_seconds: impl Fn() -> (u64, u64) + Send + Sync + 'static,
    ) -> SuccessCallback {
        Arc::new(move |op| {
            if let Some(owner) = owner.upgrade() {
                let (limit, seconds) = current_limit_and_seconds();
                owner
                    .lock()
                    .expect("checker dispatcher poisoned")
                    .record_op_success(op, limit, seconds);
            }
        })
    }
    pub fn record_op_success(&mut self, op: &Operator, affinity_limit: u64, now_seconds: u64) {
        if op.status() == SUCCESS {
            self.affinity
                .record_op_success(op, affinity_limit, now_seconds);
        }
    }
    pub fn recently_affinity_merged(&self, id: u64, now_seconds: u64) -> bool {
        self.affinity.recently_merged(id, now_seconds)
    }
    /// Heartbeat step completion is distinct from whole-operator success.
    pub fn record_region_promote_to_non_witness(
        &mut self,
        id: u64,
        now: i128,
        interval_nanos: i128,
    ) {
        self.rule_fault
            .record_region_promote_to_non_witness(id, now, interval_nanos);
    }
    /// Preserve pending on absent/unchanged regions and store-budget rejection.
    /// Queue admission failure still removes pending, matching Go AddWaitingOperator.
    pub async fn try_add_operators<C, F>(
        &mut self,
        ctx: &CheckerDispatchContext<'_, C>,
        rules: &mut RuleManager,
        affinity: &mut Manager,
        region: Option<&Arc<RegionInfo>>,
        allocate: &mut F,
    ) -> TryAddCheckersOutcome
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let mut result = TryAddCheckersOutcome::default();
        let Some(region) = region else {
            return result;
        };
        let id = region.meta().id;
        if ctx.operators.get_operator(id).is_some() {
            self.remove_pending_processed_region(id);
            return result;
        }
        let dispatch = self
            .check_region(ctx, rules, affinity, region, allocate)
            .await;
        result.errors = dispatch.errors;
        if dispatch.operators.is_empty() {
            return result;
        }
        self.enqueue_checked_operators(ctx, id, dispatch.operators, result.errors)
            .await
    }
    // Kept separate so the real budget/admission boundary can be exercised after
    // planning, including budget changes from other regions in the same batch.
    pub(crate) async fn enqueue_checked_operators<C: CheckerConfigProvider>(
        &self,
        ctx: &CheckerDispatchContext<'_, C>,
        id: u64,
        operators: Vec<Operator>,
        errors: Vec<DispatchError>,
    ) -> TryAddCheckersOutcome {
        let mut result = TryAddCheckersOutcome {
            errors,
            ..Default::default()
        };
        let ops: Vec<_> = operators.into_iter().map(Arc::new).collect();
        if exceed_store_limit(&ops, ctx.cluster, ctx.affinity.rules.replica.config).await {
            self.add_pending_processed_regions(true, &[id]);
            result.store_limited = true;
        } else {
            result.admission_error = ctx.operators.enqueue_waiting(ctx.cluster, &ops).err();
            if result.admission_error.is_none() {
                result.queued = ops;
            }
            self.remove_pending_processed_region(id);
        }
        result
    }
}
/// Observable queue/budget result; returned operators retain canonical identity.
#[derive(Default)]
pub struct TryAddCheckersOutcome {
    pub queued: Vec<Arc<Operator>>,
    pub errors: Vec<DispatchError>,
    pub store_limited: bool,
    pub admission_error: Option<AdmissionError>,
}
