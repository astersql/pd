// Copyright 2026 AsterSQL.
//! Leader balancing over borrowed canonical state, on the host runtime.
use crate::{BalanceSolver, RetryQuota};
use astersql_pd_core::{KeyRange, StoreInfo};
use astersql_pd_core_constant::{PriorityLevel, ResourceKind, ScheduleKind, SchedulePolicy};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    AffinityFilter, Filter, PlacementGuardContext, RegionDownFilter, RegionFilter,
    RegionPendingFilter, SpecialUseFilter, StoreStateFilter,
};
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpInfluence, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_plan::{BalanceSchedulerPlan, Status, StatusCode};
use astersql_pd_statistics::StatisticsSnapshot;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid batch size: expected 1..=100, got {0}")]
pub struct InvalidLeaderBatch(pub usize);
/// Validated configuration snapshot. Empty ranges mean the entire keyspace.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "LeaderConfigJson", into = "LeaderConfigJson")]
pub struct BalanceLeaderConfig {
    ranges: Vec<KeyRange>,
    batch: usize,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct LeaderConfigJson {
    ranges: Vec<astersql_pd_keyutil::KeyRange>,
    batch: usize,
}
impl From<BalanceLeaderConfig> for LeaderConfigJson {
    fn from(config: BalanceLeaderConfig) -> Self {
        Self {
            batch: config.batch,
            ranges: config
                .ranges
                .into_iter()
                .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                .collect(),
        }
    }
}
impl TryFrom<LeaderConfigJson> for BalanceLeaderConfig {
    type Error = InvalidLeaderBatch;
    fn try_from(config: LeaderConfigJson) -> Result<Self, Self::Error> {
        Self::new(
            config
                .ranges
                .into_iter()
                .map(|r| KeyRange {
                    start_key: r.start_key,
                    end_key: r.end_key,
                })
                .collect(),
            config.batch,
        )
    }
}
impl Default for BalanceLeaderConfig {
    fn default() -> Self {
        Self {
            ranges: vec![KeyRange::default()],
            batch: 4,
        }
    }
}
impl BalanceLeaderConfig {
    pub fn new(ranges: Vec<KeyRange>, batch: usize) -> Result<Self, InvalidLeaderBatch> {
        let mut c = Self::default();
        c.set_batch(batch)?;
        if !ranges.is_empty() {
            c.ranges = ranges;
        }
        Ok(c)
    }
    pub fn batch(&self) -> usize {
        self.batch
    }
    pub fn ranges(&self) -> &[KeyRange] {
        &self.ranges
    }
    pub fn set_batch(&mut self, batch: usize) -> Result<(), InvalidLeaderBatch> {
        if !(1..=100).contains(&batch) {
            return Err(InvalidLeaderBatch(batch));
        }
        self.batch = batch;
        Ok(())
    }
}
/// Candidate ordering, including Go's larger-ID tie preference and incremental resort.
pub struct LeaderCandidates {
    stores: Vec<Arc<StoreInfo>>,
    index: usize,
    asc: bool,
    policy: SchedulePolicy,
}
impl LeaderCandidates {
    pub fn new(
        stores: Vec<Arc<StoreInfo>>,
        asc: bool,
        policy: SchedulePolicy,
        influence: &OpInfluence,
    ) -> Self {
        let mut c = Self {
            stores,
            index: 0,
            asc,
            policy,
        };
        c.stores
            .sort_by(|a, b| Self::compare(a, b, asc, policy, influence));
        c
    }
    fn compare(
        a: &StoreInfo,
        b: &StoreInfo,
        asc: bool,
        policy: SchedulePolicy,
        i: &OpInfluence,
    ) -> std::cmp::Ordering {
        let kind = ScheduleKind::new(ResourceKind::LEADER, policy);
        let sa = a.leader_score(
            policy,
            i.get_store_influence(a.id()).resource_property(kind),
        );
        let sb = b.leader_score(
            policy,
            i.get_store_influence(b.id()).resource_property(kind),
        );
        if (sa - sb).abs() <= 1e-6 {
            return b.id().cmp(&a.id());
        }
        let order = sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal);
        if asc { order } else { order.reverse() }
    }
    pub fn ids(&self) -> Vec<u64> {
        self.stores.iter().map(|s| s.id()).collect()
    }
    pub fn binary_search(&self, id: u64, influence: &OpInfluence) -> usize {
        let Some(store) = self.stores.iter().find(|s| s.id() == id) else {
            return self.stores.len();
        };
        let (mut lo, mut hi) = (0, self.stores.len().saturating_sub(1));
        while lo < hi {
            let mid = (lo + hi) / 2;
            if Self::compare(&self.stores[mid], store, self.asc, self.policy, influence).is_lt() {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    pub fn resort(&mut self, mut pos: usize, i: &OpInfluence) {
        if pos >= self.stores.len() {
            return;
        }
        while pos + 1 < self.stores.len()
            && !Self::compare(
                &self.stores[pos],
                &self.stores[pos + 1],
                self.asc,
                self.policy,
                i,
            )
            .is_lt()
        {
            self.stores.swap(pos, pos + 1);
            pos += 1;
        }
        while pos > 1
            && Self::compare(
                &self.stores[pos],
                &self.stores[pos - 1],
                self.asc,
                self.policy,
                i,
            )
            .is_lt()
        {
            self.stores.swap(pos, pos - 1);
            pos -= 1;
        }
    }
    fn current(&self) -> Option<Arc<StoreInfo>> {
        self.stores.get(self.index).cloned()
    }
}
/// Immutable host observations plus the canonical mutable placement cache.
/// Statistics and affinity are real snapshots; only randomness and clock are controlled.
pub struct LeaderBalanceContext<'a, C: BasicCluster, P: SchedulerConfigProvider> {
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
}
/// Typed failures remain available to diagnostics instead of losing factory causes.
#[derive(Default)]
pub struct LeaderBalanceResult {
    pub operators: Vec<Arc<Operator>>,
    pub plans: Vec<BalanceSchedulerPlan>,
    pub errors: Vec<BuilderError>,
}
pub struct BalanceLeaderScheduler {
    pub config: BalanceLeaderConfig,
    pub name: String,
    quota: RetryQuota,
}
impl Default for BalanceLeaderScheduler {
    fn default() -> Self {
        Self {
            config: BalanceLeaderConfig::default(),
            name: "balance-leader-scheduler".into(),
            quota: RetryQuota::default(),
        }
    }
}
impl BalanceLeaderScheduler {
    pub fn is_schedule_allowed(operator_count: u64, limit: u64) -> bool {
        operator_count < limit
    }
    /// The coordinator supplies current pending influence; this generation owns its batch delta.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut LeaderBalanceContext<'_, C, P>,
        dry_run: bool,
    ) -> LeaderBalanceResult {
        let policy = ctx.config.get_leader_schedule_policy();
        let kind = ScheduleKind::new(ResourceKind::LEADER, policy);
        let stores = ctx.cluster.get_stores();
        let state = StoreStateFilter {
            action_scope: self.name.clone(),
            transfer_leader: true,
            operator_level: PriorityLevel::HIGH,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new(&self.name, &[]);
        let mut sources = Vec::new();
        let mut targets = Vec::new();
        let mut result = LeaderBalanceResult::default();
        for store in &stores {
            let status = state.source(ctx.config, store).await;
            let status = if status.is_ok() {
                special.source(ctx.config, store)
            } else {
                status
            };
            if status.is_ok() {
                sources.push(store.clone());
            } else if dry_run {
                result.plans.push(BalanceSchedulerPlan {
                    source: Some(store.clone()),
                    status,
                    ..Default::default()
                });
            }
            if state.target(ctx.config, store).await.is_ok()
                && special.target(ctx.config, store).is_ok()
            {
                targets.push(store.clone());
            }
        }
        let influence = OpInfluence::new();
        influence.add(ctx.influence);
        let mut source = LeaderCandidates::new(sources, false, policy, &influence);
        let mut target = LeaderCandidates::new(targets, true, policy, &influence);
        let mut used = HashSet::new();
        let solver = BalanceSolver::new(
            kind,
            ctx.config.get_schedule_config(),
            ctx.cluster.average_region_size(),
            stores
                .iter()
                .map(|s| ctx.cluster.store_region_count(s.id()))
                .max()
                .unwrap_or(0),
        );
        let ranges = if self.name == "balance-leader-scheduler" && !ctx.key_ranges.is_empty() {
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
        while source.current().is_some() || target.current().is_some() {
            for out in [true, false] {
                let candidate = if out { &mut source } else { &mut target };
                let Some(store) = candidate.current() else {
                    continue;
                };
                let mut found = None;
                for _ in 0..self.quota.get_limit(store.id()) {
                    let regions = if out {
                        ctx.cluster.rand_leader_regions(
                            store.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        )
                    } else {
                        ctx.cluster.rand_follower_regions(
                            store.id(),
                            &ranges,
                            ctx.random_index,
                            ctx.shuffle,
                        )
                    };
                    let mut selected = None;
                    for region in regions {
                        let mut status = RegionPendingFilter.select(&region);
                        if status.is_ok() {
                            status = RegionDownFilter.select(&region);
                        }
                        if status.is_ok() {
                            status = ctx.affinity.select(&region);
                        }
                        if status.is_ok() {
                            selected = Some(region);
                            break;
                        }
                        if dry_run && out {
                            result.plans.push(BalanceSchedulerPlan {
                                source: Some(store.clone()),
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
                    if ctx.statistics.is_region_hot(&region, ctx.hot_degree) {
                        if dry_run && out {
                            result.plans.push(BalanceSchedulerPlan {
                                source: Some(store.clone()),
                                region: Some(region.clone()),
                                step: 1,
                                status: Status::new(StatusCode::REGION_HOT, &[]),
                                ..Default::default()
                            });
                        }
                        continue;
                    }
                    let Some(src) = ctx.cluster.get_leader_store(&region) else {
                        continue;
                    };
                    if !state.source(ctx.config, &src).await.is_ok()
                        || !special.source(ctx.config, &src).is_ok()
                    {
                        continue;
                    }
                    let mut followers = if out {
                        ctx.cluster.get_follower_stores(&region)
                    } else {
                        vec![store.clone()]
                    };
                    // Go's follower ordering uses raw score comparison; the epsilon/ID
                    // tie rule belongs only to the outer source/target candidate queues.
                    followers.sort_by(|a, b| {
                        let score = |s: &StoreInfo| {
                            s.leader_score(
                                policy,
                                influence
                                    .get_store_influence(s.id())
                                    .resource_property(kind),
                            )
                        };
                        score(a)
                            .partial_cmp(&score(b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let mut guard = PlacementGuardContext {
                        conf: ctx.config,
                        stores: &stores,
                        manager: ctx.rules,
                        region: region.clone(),
                        now: ctx.now_unix_nanos,
                    };
                    let leader_guard = guard.leader_safeguard(&self.name, src.id(), None, false);
                    for dst in followers {
                        let mut status = state.target(ctx.config, &dst).await;
                        if status.is_ok() {
                            status = special.target(ctx.config, &dst);
                        }
                        if status.is_ok()
                            && let Some(guard) = &leader_guard
                        {
                            status = guard.target(ctx.config, &dst);
                        }
                        if !status.is_ok() {
                            if dry_run && out {
                                result.plans.push(BalanceSchedulerPlan {
                                    source: Some(src.clone()),
                                    region: Some(region.clone()),
                                    target: Some(dst),
                                    step: 2,
                                    status,
                                });
                            }
                            continue;
                        }
                        let mut plan = BalanceSchedulerPlan {
                            source: Some(src.clone()),
                            target: Some(dst.clone()),
                            region: Some(region.clone()),
                            step: 3,
                            ..Default::default()
                        };
                        let scores = solver
                            .evaluate(&plan, &influence)
                            .expect("selected source and target");
                        if !scores.should_balance() {
                            if dry_run && out {
                                plan.status = Status::new(StatusCode::STORE_SCORE_DISALLOWED, &[]);
                                result.plans.push(plan);
                            }
                            continue;
                        }
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
                            false,
                        );
                        match factory
                            .create_transfer_leader(dst.id(), vec![], OpKind::LEADER, || {
                                Err(BuilderError::Allocation(
                                    "leader transfer must not allocate".into(),
                                ))
                            })
                            .await
                        {
                            Ok(op) => {
                                if used.contains(&region.meta().id) {
                                    break;
                                }
                                op.additional_info()
                                    .set("sourceScore", format!("{:.2}", scores.source_score));
                                op.additional_info()
                                    .set("targetScore", format!("{:.2}", scores.target_score));
                                op.set_store_health_check(true);
                                found = Some((Arc::new(op), region.clone(), src.id(), dst.id()));
                                break;
                            }
                            Err(e) => {
                                result.errors.push(e);
                                if dry_run && out {
                                    plan.step = 4;
                                    plan.status =
                                        Status::new(StatusCode::CREATE_OPERATOR_FAILED, &[]);
                                    result.plans.push(plan);
                                }
                            }
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
                if let Some((op, region, src, dst)) = found {
                    self.quota.reset_limit(store.id());
                    result.operators.push(op.clone());
                    if result.operators.len() >= self.config.batch {
                        return result;
                    }
                    used.insert(region.meta().id);
                    let positions = [
                        source.binary_search(src, &influence),
                        source.binary_search(dst, &influence),
                        target.binary_search(src, &influence),
                        target.binary_search(dst, &influence),
                    ];
                    op.total_influence(&influence, Some(&region));
                    source.resort(positions[0], &influence);
                    source.resort(positions[1], &influence);
                    target.resort(positions[2], &influence);
                    target.resort(positions[3], &influence);
                } else {
                    self.quota.attenuate(store.id());
                    if out {
                        source.index += 1;
                    } else {
                        target.index += 1;
                    }
                }
            }
        }
        self.quota.gc(&[source.ids(), target.ids()].concat());
        result
    }
}
