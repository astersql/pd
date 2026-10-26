// Copyright 2026 AsterSQL.
//! Region peer balancing over canonical state, placement and operator builders.
use crate::{BalanceSolver, RetryQuota};
use astersql_pd_core::{KeyRange, Peer};
use astersql_pd_core_constant::{PriorityLevel, ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    AffinityFilter, Filter, PlacementGuardContext, RegionDownFilter, RegionEmptyFilter,
    RegionFilter, RegionPendingFilter, RegionReplicatedFilter, RegionReplicationContext,
    RegionWitnessFilter, SnapshotSenderFilter, SpecialUseFilter, StoreStateFilter,
};
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpInfluence, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Status, StatusCode};
use astersql_pd_statistics::StatisticsSnapshot;
use std::{cell::RefCell, collections::HashSet, sync::Arc};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(from = "RegionConfigJson", into = "RegionConfigJson")]
pub struct BalanceRegionConfig {
    ranges: Vec<KeyRange>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct RegionConfigJson {
    ranges: Vec<astersql_pd_keyutil::KeyRange>,
}
impl From<RegionConfigJson> for BalanceRegionConfig {
    fn from(c: RegionConfigJson) -> Self {
        Self::new(
            c.ranges
                .into_iter()
                .map(|r| KeyRange {
                    start_key: r.start_key,
                    end_key: r.end_key,
                })
                .collect(),
        )
    }
}
impl From<BalanceRegionConfig> for RegionConfigJson {
    fn from(c: BalanceRegionConfig) -> Self {
        Self {
            ranges: c
                .ranges
                .into_iter()
                .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                .collect(),
        }
    }
}
impl Default for BalanceRegionConfig {
    fn default() -> Self {
        Self::new(vec![])
    }
}
impl BalanceRegionConfig {
    pub fn new(mut ranges: Vec<KeyRange>) -> Self {
        if ranges.is_empty() {
            ranges.push(KeyRange::default());
        }
        Self { ranges }
    }
    pub fn ranges(&self) -> &[KeyRange] {
        &self.ranges
    }
}
/// Host-owned services. The allocator is the only fallible external write boundary.
/// A range-view host explicitly permits empty regions, matching Go rangeCluster.
pub struct RegionBalanceContext<'a, C: BasicCluster, P: SchedulerConfigProvider> {
    pub cluster: &'a C,
    pub config: &'a P,
    pub rules: &'a mut RuleManager,
    pub affinity: AffinityFilter<'a>,
    pub statistics: &'a StatisticsSnapshot,
    pub hot_degree: i64,
    pub key_ranges: &'a astersql_pd_schedule_keyrange::Manager,
    pub influence: &'a OpInfluence,
    pub now_unix_nanos: i128,
    pub random_index: &'a mut dyn FnMut(usize) -> usize,
    pub shuffle: &'a mut dyn FnMut(&mut [usize]),
    pub allocate: &'a mut dyn FnMut() -> Result<u64, BuilderError>,
    pub supports_conf_change_v2: bool,
    pub allow_empty_regions: bool,
}
#[derive(Default)]
pub struct RegionBalanceResult {
    pub operators: Vec<Arc<Operator>>,
    pub plans: Vec<BalanceSchedulerPlan>,
    pub errors: Vec<BuilderError>,
}
pub struct BalanceRegionScheduler {
    pub config: BalanceRegionConfig,
    pub name: String,
    quota: RetryQuota,
}
impl Default for BalanceRegionScheduler {
    fn default() -> Self {
        Self {
            config: Default::default(),
            name: "balance-region-scheduler".into(),
            quota: Default::default(),
        }
    }
}
impl BalanceRegionScheduler {
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut RegionBalanceContext<'_, C, P>,
        dry_run: bool,
    ) -> RegionBalanceResult {
        let mut result = RegionBalanceResult::default();
        let stores = ctx.cluster.get_stores();
        let influence = OpInfluence::new();
        influence.add(ctx.influence);
        let state = StoreStateFilter {
            action_scope: self.name.clone(),
            move_region: true,
            operator_level: PriorityLevel::MEDIUM,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new(&self.name, &[]);
        let mut sources = Vec::new();
        let mut unavailable = HashSet::new();
        for s in &stores {
            if !state.target(ctx.config, s).await.is_ok() || !special.target(ctx.config, s).is_ok()
            {
                unavailable.insert(s.id());
            }
            let mut status = state.source(ctx.config, s).await;
            if status.is_ok() {
                status = special.source(ctx.config, s);
            }
            if status.is_ok() {
                sources.push(s.clone());
            } else if dry_run {
                result.plans.push(BalanceSchedulerPlan {
                    source: Some(s.clone()),
                    status,
                    ..Default::default()
                });
            }
        }
        let kind = ScheduleKind::new(ResourceKind::REGION, SchedulePolicy::BY_SIZE);
        let conf = ctx.config.get_schedule_config();
        sources.sort_by(|a, b| {
            let score = |s: &astersql_pd_core::StoreInfo| {
                s.region_score(
                    &conf.region_score_formula_version,
                    conf.high_space_ratio,
                    conf.low_space_ratio,
                    influence
                        .get_store_influence(s.id())
                        .resource_property(kind),
                )
            };
            score(b)
                .partial_cmp(&score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if dry_run && let Some(source) = sources.first() {
            result.plans.push(BalanceSchedulerPlan {
                source: Some(source.clone()),
                status: Status::new(StatusCode::STORE_SCORE_DISALLOWED, &[]),
                ..Default::default()
            });
        }
        let solver = BalanceSolver::new(
            kind,
            conf,
            ctx.cluster.average_region_size(),
            stores
                .iter()
                .map(|s| ctx.cluster.store_region_count(s.id()))
                .max()
                .unwrap_or(0),
        );
        let snapshot = SnapshotSenderFilter::new(&stores, PriorityLevel::MEDIUM).await;
        let ranges = if self.name == "balance-region-scheduler" && !ctx.key_ranges.is_empty() {
            ctx.key_ranges
                .get_non_overlapping_key_ranges(&astersql_pd_keyutil::KeyRange::new(
                    &self.config.ranges[0].start_key,
                    &self.config.ranges[0].end_key,
                ))
                .into_iter()
                .map(|r| KeyRange {
                    start_key: r.start_key,
                    end_key: r.end_key,
                })
                .collect()
        } else {
            self.config.ranges.clone()
        };
        for (source_index, source) in sources
            .iter()
            .enumerate()
            .take(sources.len().saturating_sub(1))
        {
            for _ in 0..self.quota.get_limit(source.id()) {
                let mut selected = None;
                // Preserve pending, follower, leader, learner priority without sampling later groups early.
                for group in 0..4 {
                    let regions = match group {
                        0 => ctx.cluster.rand_pending_regions(
                            source.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        ),
                        1 => ctx.cluster.rand_follower_regions(
                            source.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        ),
                        2 => ctx.cluster.rand_leader_regions(
                            source.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        ),
                        _ => ctx.cluster.rand_learner_regions(
                            source.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        ),
                    };
                    let replica = RegionReplicatedFilter::new(RegionReplicationContext {
                        conf: ctx.config,
                        stores: &stores,
                        manager: RefCell::new(&mut *ctx.rules),
                        now: ctx.now_unix_nanos,
                    });
                    for region in regions {
                        let mut status = RegionDownFilter.select(&region);
                        if status.is_ok() {
                            status = replica.select(&region);
                        }
                        if status.is_ok() {
                            status = snapshot.select(&region);
                        }
                        if status.is_ok() {
                            status = ctx.affinity.select(&region);
                        }
                        if status.is_ok() && !ctx.allow_empty_regions {
                            status = RegionEmptyFilter {
                                total_region_count: ctx.cluster.total_region_count() as i64,
                            }
                            .select(&region);
                        }
                        if status.is_ok() {
                            status = RegionWitnessFilter {
                                store_id: source.id(),
                            }
                            .select(&region);
                        }
                        if status.is_ok() && group != 0 {
                            status = RegionPendingFilter.select(&region);
                        }
                        if status.is_ok() {
                            selected = Some((region, replica.get_fit()));
                            break;
                        }
                        if dry_run {
                            result.plans.push(BalanceSchedulerPlan {
                                source: Some(source.clone()),
                                region: Some(region),
                                step: 1,
                                status,
                                ..Default::default()
                            });
                        }
                    }
                    if selected.is_some() {
                        break;
                    }
                }
                let Some((region, fit)) = selected else {
                    continue;
                };
                let mut plan = BalanceSchedulerPlan {
                    source: Some(source.clone()),
                    region: Some(region.clone()),
                    step: 1,
                    ..Default::default()
                };
                if ctx.statistics.is_region_hot(&region, ctx.hot_degree) {
                    plan.status = Status::new(StatusCode::REGION_HOT, &[]);
                    if dry_run {
                        result.plans.push(plan);
                    }
                    continue;
                }
                if region.leader().is_none() {
                    plan.status = Status::new(StatusCode::REGION_NO_LEADER, &[]);
                    if dry_run {
                        result.plans.push(plan);
                    }
                    continue;
                }
                let guard = PlacementGuardContext {
                    conf: ctx.config,
                    stores: &stores,
                    manager: ctx.rules,
                    region: region.clone(),
                    now: ctx.now_unix_nanos,
                }
                .safeguard(&self.name, source, fit);
                for target in sources[source_index + 1..].iter().rev() {
                    if region.get_store_peer(target.id()).is_some()
                        || unavailable.contains(&target.id())
                    {
                        continue;
                    }
                    plan.target = Some(target.clone());
                    plan.step = 2;
                    let status = guard.target(ctx.config, target);
                    if !status.is_ok() {
                        if dry_run {
                            let mut p = plan.clone();
                            p.status = status;
                            result.plans.push(p);
                        }
                        continue;
                    }
                    plan.step = 3;
                    let scores = solver
                        .evaluate(&plan, &influence)
                        .expect("selected source and target");
                    if !scores.should_balance() {
                        if dry_run {
                            let mut p = plan.clone();
                            p.status = Status::new(StatusCode::STORE_SCORE_DISALLOWED, &[]);
                            result.plans.push(p);
                        }
                        continue;
                    }
                    let old = region
                        .get_store_peer(source.id())
                        .expect("sampled source peer");
                    let rules = ctx.rules.get_rules_for_apply_range(
                        &region.meta().key_range.start_key,
                        &region.meta().key_range.end_key,
                    );
                    let factory = PeerOperatorFactory::new(
                        &self.name,
                        BuilderContext {
                            stores: ctx.cluster.stores(),
                            config: ctx.config,
                            rules: &rules,
                            now_unix_nanos: ctx.now_unix_nanos,
                        },
                        &region,
                        ctx.supports_conf_change_v2,
                    );
                    plan.step = 4;
                    match factory
                        .create_move_peer(
                            source.id(),
                            Some(Peer {
                                store_id: target.id(),
                                role: old.role,
                                ..Default::default()
                            }),
                            OpKind::REGION,
                            &mut *ctx.allocate,
                        )
                        .await
                    {
                        Ok(op) => {
                            op.additional_info()
                                .set("sourceScore", format!("{:.2}", scores.source_score));
                            op.additional_info()
                                .set("targetScore", format!("{:.2}", scores.target_score));
                            op.set_store_health_check(true);
                            if dry_run {
                                result.plans.push(plan);
                            }
                            result.operators.push(Arc::new(op));
                            self.quota.reset_limit(source.id());
                            return result;
                        }
                        Err(e) => {
                            result.errors.push(e);
                            plan.status = Status::new(StatusCode::CREATE_OPERATOR_FAILED, &[]);
                            if dry_run {
                                result.plans.push(plan.clone());
                            }
                            break;
                        }
                    }
                }
            }
            self.quota.attenuate(source.id());
        }
        self.quota
            .gc(&stores.iter().map(|s| s.id()).collect::<Vec<_>>());
        result
    }
}
