// Copyright 2026 AsterSQL.
//! Balance witness roles among existing voters using canonical placement and operators.
use crate::{BalanceRegionConfig, BalanceSolver, RegionBalanceResult, RetryQuota};
use astersql_pd_core::KeyRange;
use astersql_pd_core_constant::{PriorityLevel, ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    Filter, PlacementGuardContext, RegionDownFilter, RegionFilter, RegionPendingFilter,
    SpecialUseFilter, StoreStateFilter,
};
use astersql_pd_schedule_operator::{BuilderContext, OpInfluence, RangeOperatorFactory};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Status, StatusCode};
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "WitnessConfigJson", into = "WitnessConfigJson")]
pub struct BalanceWitnessConfig {
    ranges: Option<Vec<KeyRange>>,
    batch: i64,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct WitnessConfigJson {
    ranges: Option<Vec<astersql_pd_keyutil::KeyRange>>,
    batch: i64,
}
impl TryFrom<WitnessConfigJson> for BalanceWitnessConfig {
    type Error = WitnessConfigError;
    fn try_from(c: WitnessConfigJson) -> Result<Self, Self::Error> {
        if !(1..=10).contains(&c.batch) {
            return Err(WitnessConfigError);
        }
        Ok(Self::from_transport_parts(
            c.ranges.map(|ranges| {
                ranges
                    .into_iter()
                    .map(|r| KeyRange {
                        start_key: r.start_key,
                        end_key: r.end_key,
                    })
                    .collect()
            }),
            c.batch,
        ))
    }
}
impl From<BalanceWitnessConfig> for WitnessConfigJson {
    fn from(c: BalanceWitnessConfig) -> Self {
        Self {
            ranges: c.ranges.map(|ranges| {
                ranges
                    .into_iter()
                    .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                    .collect()
            }),
            batch: c.batch,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid batch size which should be an integer between 1 and 10")]
pub struct WitnessConfigError;
impl Default for BalanceWitnessConfig {
    fn default() -> Self {
        Self::new(vec![], 4).expect("valid default batch")
    }
}
impl BalanceWitnessConfig {
    pub fn new(ranges: Vec<KeyRange>, batch: usize) -> Result<Self, WitnessConfigError> {
        if !(1..=10).contains(&batch) {
            return Err(WitnessConfigError);
        }
        Ok(Self {
            ranges: Some(BalanceRegionConfig::new(ranges).ranges().to_vec()),
            batch: batch as i64,
        })
    }
    /// Preserve the exact transport snapshot, including null/empty ranges and
    /// partially decoded values on Go-compatible HTTP errors. Validated callers
    /// should use `new`; this path intentionally does not validate or persist.
    pub fn from_transport_parts(ranges: Option<Vec<KeyRange>>, batch: i64) -> Self {
        Self { ranges, batch }
    }
    /// Exact signed transport batch; a failed Go decode may leave it invalid.
    pub fn transport_batch(&self) -> i64 {
        self.batch
    }
    /// Distinguish null ranges from an explicitly empty list at the HTTP boundary.
    pub fn transport_ranges(&self) -> Option<&[KeyRange]> {
        self.ranges.as_deref()
    }
    pub fn batch(&self) -> usize {
        self.batch.try_into().unwrap_or(0)
    }
    pub fn ranges(&self) -> &[KeyRange] {
        self.ranges.as_deref().unwrap_or_default()
    }
}
/// Immutable host snapshot, with explicit clock and region sampling boundaries.
pub struct WitnessBalanceContext<'a, C: BasicCluster, P: SchedulerConfigProvider> {
    pub cluster: &'a C,
    pub config: &'a P,
    pub rules: &'a mut RuleManager,
    pub influence: &'a OpInfluence,
    pub now_unix_nanos: i128,
    pub random_index: &'a mut dyn FnMut(usize) -> usize,
    pub shuffle: &'a mut dyn FnMut(&mut [usize]),
    pub supports_conf_change_v2: bool,
}
pub struct BalanceWitnessScheduler {
    pub config: BalanceWitnessConfig,
    quota: RetryQuota,
}
impl Default for BalanceWitnessScheduler {
    fn default() -> Self {
        Self {
            config: Default::default(),
            quota: Default::default(),
        }
    }
}
impl BalanceWitnessScheduler {
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    /// Go selects only the highest scoring source and emits at most one operation,
    /// even when the configured batch is larger. Admission is queried separately.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut WitnessBalanceContext<'_, C, P>,
        dry_run: bool,
    ) -> RegionBalanceResult {
        let mut out = RegionBalanceResult::default();
        let stores = ctx.cluster.get_stores();
        let influence = OpInfluence::new();
        influence.add(ctx.influence);
        let name = "balance-witness-scheduler";
        let state = StoreStateFilter {
            action_scope: name.into(),
            move_region: true,
            operator_level: PriorityLevel::MEDIUM,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new(name, &[]);
        let mut sources = Vec::new();
        for s in &stores {
            let mut status = state.source(ctx.config, s).await;
            if status.is_ok() {
                status = special.source(ctx.config, s);
            }
            if status.is_ok() {
                sources.push(s.clone());
            } else if dry_run {
                out.plans.push(BalanceSchedulerPlan {
                    source: Some(s.clone()),
                    status,
                    ..Default::default()
                });
            }
        }
        sources.sort_by(|a, b| {
            b.witness_score(influence.get_store_influence(b.id()).witness_count)
                .total_cmp(&a.witness_score(influence.get_store_influence(a.id()).witness_count))
        });
        let Some(source) = sources.first() else {
            return out;
        };
        let solver = BalanceSolver::new(
            ScheduleKind::new(ResourceKind::WITNESS, SchedulePolicy::BY_COUNT),
            ctx.config.get_schedule_config(),
            ctx.cluster.average_region_size(),
            stores
                .iter()
                .map(|s| ctx.cluster.store_region_count(s.id()))
                .max()
                .unwrap_or(0),
        );
        for _ in 0..self.quota.get_limit(source.id()) {
            let regions = ctx.cluster.rand_witness_regions(
                source.id(),
                self.config.ranges(),
                ctx.random_index,
                ctx.shuffle,
            );
            let mut selected = None;
            for region in regions {
                let mut status = RegionPendingFilter.select(&region);
                if status.is_ok() {
                    status = RegionDownFilter.select(&region);
                }
                if status.is_ok() {
                    selected = Some(region);
                    break;
                }
                if dry_run {
                    out.plans.push(BalanceSchedulerPlan {
                        source: Some(source.clone()),
                        region: Some(region),
                        step: 1,
                        status,
                        ..Default::default()
                    });
                }
            }
            let Some(region) = selected else {
                continue;
            };
            let guard = PlacementGuardContext {
                conf: ctx.config,
                stores: &stores,
                manager: ctx.rules,
                region: region.clone(),
                now: ctx.now_unix_nanos,
            }
            .witness_safeguard(name, source.id(), None);
            let mut targets = ctx.cluster.get_non_witness_voter_stores(&region);
            targets.sort_by(|a, b| {
                a.witness_score(influence.get_store_influence(a.id()).witness_count)
                    .total_cmp(
                        &b.witness_score(influence.get_store_influence(b.id()).witness_count),
                    )
            });
            for target in targets {
                let mut plan = BalanceSchedulerPlan {
                    source: Some(source.clone()),
                    target: Some(target.clone()),
                    region: Some(region.clone()),
                    step: 2,
                    ..Default::default()
                };
                let mut status = state.target(ctx.config, &target).await;
                if status.is_ok() {
                    status = special.target(ctx.config, &target);
                }
                if status.is_ok()
                    && let Some(guard) = &guard
                {
                    status = guard.target(ctx.config, &target);
                }
                if !status.is_ok() {
                    if dry_run {
                        plan.status = status;
                        out.plans.push(plan);
                    }
                    continue;
                }
                plan.step = 3;
                let scores = solver
                    .evaluate(&plan, &influence)
                    .expect("selected endpoints");
                if !scores.should_balance() {
                    if dry_run {
                        plan.status = Status::new(StatusCode::STORE_SCORE_DISALLOWED, &[]);
                        out.plans.push(plan);
                    }
                    continue;
                }
                plan.step = 4;
                let rules = ctx.rules.get_rules_for_apply_range(
                    &region.meta().key_range.start_key,
                    &region.meta().key_range.end_key,
                );
                // Role switches preserve existing peer IDs and never allocate a peer.
                let factory = RangeOperatorFactory::new(
                    name,
                    BuilderContext {
                        stores: ctx.cluster.stores(),
                        config: ctx.config,
                        rules: &rules,
                        now_unix_nanos: ctx.now_unix_nanos,
                    },
                    &region,
                    ctx.supports_conf_change_v2,
                );
                match factory
                    .create_move_witness(source.id(), target.id(), || {
                        Err(astersql_pd_schedule_operator::BuilderError::Allocation(
                            "unexpected witness allocation".into(),
                        ))
                    })
                    .await
                {
                    Ok(op) => {
                        op.additional_info()
                            .set("sourceScore", format!("{:.2}", scores.source_score));
                        op.additional_info()
                            .set("targetScore", format!("{:.2}", scores.target_score));
                        out.operators.push(Arc::new(op));
                        if dry_run {
                            out.plans.push(plan);
                        }
                        self.quota.reset_limit(source.id());
                        return out;
                    }
                    Err(e) => {
                        out.errors.push(e);
                        if dry_run {
                            plan.status = Status::new(StatusCode::CREATE_OPERATOR_FAILED, &[]);
                            out.plans.push(plan);
                        }
                    }
                }
            }
        }
        self.quota.attenuate(source.id());
        self.quota
            .gc(&sources.iter().map(|s| s.id()).collect::<Vec<_>>());
        out
    }
}
