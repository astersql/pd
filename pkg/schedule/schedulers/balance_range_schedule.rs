// Copyright 2026 AsterSQL.
//! Sequential range jobs over canonical state, placement, filtering and builders.
use crate::{
    BalanceRangeJob, BalanceRangeJobs, JobStatus, JobTime, RangeJobError, RegionBalanceContext,
    RegionBalanceResult, get_count_threshold,
};
use astersql_pd_core::{KeyRange, Peer, Rule, StoreInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    Filter, PlacementGuardContext, RegionDownFilter, RegionPendingFilter, RegionReplicatedFilter,
    RegionReplicationContext, SnapshotSenderFilter, SpecialUseFilter, StorageThresholdFilter,
    StoreStateFilter, select_one_region,
};
use astersql_pd_schedule_operator::{BuilderContext, OpKind, PeerOperatorFactory};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
const NAME: &str = "balance-range-scheduler";
#[derive(Debug, thiserror::Error)]
pub enum BalanceRangeError {
    #[error(transparent)]
    Job(#[from] RangeJobError),
    #[error("invalid engine or rule")]
    InvalidJob,
    #[error("no store to be selected")]
    NoStores,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeReport {
    pub job_id: u64,
    pub scores: BTreeMap<u64, (f64, f64)>,
}
/// Host supplies range-scoped influence, time and allocation. Admission must run
/// before schedule; returned operations remain host-owned until submission.
#[derive(Default)]
pub struct BalanceRangeScheduler {
    pub scores: BTreeMap<u64, f64>,
    pub expected: BTreeMap<u64, f64>,
    stores: Vec<Arc<StoreInfo>>,
    job: Option<BalanceRangeJob>,
    last_report: Option<JobTime>,
    pub report: RangeReport,
}
impl BalanceRangeScheduler {
    pub fn should_balance(
        source: f64,
        target: f64,
        source_influence: i64,
        target_influence: i64,
    ) -> bool {
        source - source_influence.unsigned_abs() as f64 - 1.0
            >= target + target_influence.unsigned_abs() as f64 + 1.0
    }
    pub fn is_balanced(&self) -> bool {
        self.scores
            .iter()
            .filter_map(|(id, s)| self.expected.get(id).map(|e| (e - s).abs() as i64))
            .sum::<i64>()
            <= 1
    }
    fn clean(&mut self, ranges: &astersql_pd_schedule_keyrange::Manager, job: &BalanceRangeJob) {
        ranges.delete(&job.ranges);
        for id in self.scores.keys() {
            self.report.scores.insert(*id, (0.0, 0.0));
        }
    }
    pub fn report_at(&mut self, now: JobTime) -> bool {
        if self
            .last_report
            .is_some_and(|last| now.0 - last.0 < 10_000_000_000)
        {
            return false;
        }
        let Some(job) = &self.job else {
            return false;
        };
        self.report.job_id = job.job_id;
        for (id, score) in &self.scores {
            self.report
                .scores
                .insert(*id, (*score, *self.expected.get(id).unwrap_or(&0.0)));
        }
        self.last_report = Some(now);
        true
    }
    pub async fn prepare<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &RegionBalanceContext<'_, C, P>,
        job: &BalanceRangeJob,
    ) -> Result<(), BalanceRangeError> {
        if !matches!(job.engine.as_str(), "tikv" | "tiflash") || job.rule == Rule::Unknown {
            return Err(BalanceRangeError::InvalidJob);
        }
        let state = StoreStateFilter {
            action_scope: NAME.into(),
            transfer_leader: true,
            move_region: true,
            allow_temporary_states: true,
            operator_level: PriorityLevel::MEDIUM,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let storage = StorageThresholdFilter::new(NAME);
        let mut available = vec![];
        for s in ctx.cluster.get_stores() {
            let flash = s.label_value("engine") == "tiflash";
            if flash == (job.engine == "tiflash")
                && state.source(ctx.config, &s).await.is_ok()
                && storage.source(ctx.config, &s).is_ok()
            {
                available.push(s);
            }
        }
        available.sort_by_key(|s| s.id());
        let mut stores = vec![];
        let mut scores = BTreeMap::new();
        let mut expected = BTreeMap::new();
        for s in &available {
            let expect: f64 = job
                .ranges
                .iter()
                .map(|r| {
                    get_count_threshold(
                        ctx.cluster,
                        &available,
                        s,
                        &KeyRange {
                            start_key: r.start_key.clone(),
                            end_key: r.end_key.clone(),
                        },
                        job.rule,
                        &ctx.config.snapshot().replication,
                        ctx.rules,
                    )
                })
                .sum();
            if expect <= 0.0 {
                continue;
            }
            let score: isize = job
                .ranges
                .iter()
                .map(|r| match job.rule {
                    Rule::LeaderScatter => {
                        ctx.cluster
                            .store_leader_count_by_range(s.id(), &r.start_key, &r.end_key)
                    }
                    Rule::LearnerScatter => {
                        ctx.cluster
                            .store_learner_count_by_range(s.id(), &r.start_key, &r.end_key)
                    }
                    _ => ctx
                        .cluster
                        .store_peer_count_by_range(s.id(), &r.start_key, &r.end_key),
                })
                .sum();
            stores.push(s.clone());
            scores.insert(s.id(), score as f64);
            expected.insert(s.id(), expect);
        }
        if stores.len() <= 1 {
            return Err(BalanceRangeError::NoStores);
        }
        stores.sort_by(|a, b| {
            (scores[&b.id()] + Self::influence(ctx, job.rule, b.id()) as f64)
                .total_cmp(&(scores[&a.id()] + Self::influence(ctx, job.rule, a.id()) as f64))
        });
        self.stores = stores;
        self.scores = scores;
        self.expected = expected;
        self.job = Some(job.clone());
        self.report_at(JobTime(ctx.now_unix_nanos));
        Ok(())
    }
    fn influence<C: BasicCluster, P: SchedulerConfigProvider>(
        ctx: &RegionBalanceContext<'_, C, P>,
        rule: Rule,
        id: u64,
    ) -> i64 {
        let i = ctx.influence.get_store_influence(id);
        if rule == Rule::LeaderScatter {
            i.leader_count
        } else {
            i.region_count
        }
    }
    pub async fn check_job<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        jobs: &mut BalanceRangeJobs,
        ctx: &RegionBalanceContext<'_, C, P>,
        range_count: u64,
    ) -> Result<bool, BalanceRangeError> {
        if range_count >= ctx.config.get_region_schedule_limit() {
            return Ok(false);
        }
        let now = JobTime(ctx.now_unix_nanos);
        jobs.gc(now).await?;
        if let Some(old) = self.job.clone() {
            if jobs
                .jobs()
                .iter()
                .any(|j| j.job_id == old.job_id && j.status == JobStatus::Cancelled)
            {
                self.clean(ctx.key_ranges, &old);
                self.job = None;
            }
        }
        let Some((index, job)) = jobs.peek().map(|(i, j)| (i, j.clone())) else {
            return Ok(false);
        };
        if job.status == JobStatus::Pending {
            jobs.begin(index, now).await?;
            ctx.key_ranges.append(&job.ranges);
        }
        let job = jobs.jobs()[index].clone();
        if job.should_finish(now)? {
            jobs.finish(index, now).await?;
            self.clean(ctx.key_ranges, &job);
        }
        self.prepare(ctx, &job).await?;
        if self.is_balanced() {
            jobs.finish(index, now).await?;
            self.clean(ctx.key_ranges, &job);
            return Ok(false);
        }
        Ok(true)
    }
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut RegionBalanceContext<'_, C, P>,
    ) -> RegionBalanceResult {
        let mut out = RegionBalanceResult::default();
        let Some(job) = &self.job else {
            return out;
        };
        let state = StoreStateFilter {
            action_scope: NAME.into(),
            transfer_leader: true,
            move_region: true,
            operator_level: PriorityLevel::MEDIUM,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new(NAME, &[]);
        let mut sources = vec![];
        for s in &self.stores {
            if state.source(ctx.config, s).await.is_ok() && special.source(ctx.config, s).is_ok() {
                sources.push(s);
            }
        }
        let ranges: Vec<_> = job
            .ranges
            .iter()
            .map(|r| KeyRange {
                start_key: r.start_key.clone(),
                end_key: r.end_key.clone(),
            })
            .collect();
        let all = ctx.cluster.get_stores();
        let snapshot = SnapshotSenderFilter::new(&all, PriorityLevel::MEDIUM).await;
        for (i, source) in sources.iter().enumerate() {
            if self.scores[&source.id()] <= self.expected[&source.id()] {
                continue;
            }
            let groups: &[u8] = match job.rule {
                Rule::LeaderScatter => &[1],
                Rule::LearnerScatter => &[2],
                _ => &[0, 1, 2],
            };
            let mut selected = None;
            for group in groups {
                let regions = match group {
                    0 => ctx.cluster.rand_follower_regions(
                        source.id(),
                        &ranges,
                        ctx.random_index,
                        ctx.shuffle,
                    ),
                    1 => ctx.cluster.rand_leader_regions(
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
                    stores: &all,
                    manager: RefCell::new(&mut *ctx.rules),
                    now: ctx.now_unix_nanos,
                });
                selected = select_one_region(
                    &regions,
                    None,
                    &[&RegionDownFilter, &snapshot, &RegionPendingFilter, &replica],
                );
                if selected.is_some() {
                    break;
                }
            }
            let Some(region) = selected else {
                continue;
            };
            if region.leader().is_none() || ctx.statistics.is_region_hot(&region, ctx.hot_degree) {
                continue;
            }
            let guard = PlacementGuardContext {
                conf: ctx.config,
                stores: &all,
                manager: ctx.rules,
                region: region.clone(),
                now: ctx.now_unix_nanos,
            }
            .safeguard(NAME, source, None);
            for target in sources[i + 1..].iter().rev() {
                if self.scores[&target.id()] >= self.expected[&target.id()]
                    || !Self::should_balance(
                        self.scores[&source.id()],
                        self.scores[&target.id()],
                        Self::influence(ctx, job.rule, source.id()),
                        Self::influence(ctx, job.rule, target.id()),
                    )
                {
                    continue;
                }
                let existing = region.get_store_peer(target.id()).is_some();
                if (job.rule != Rule::LeaderScatter && existing)
                    || region.leader().is_some_and(|p| p.store_id == target.id())
                    || !state.target(ctx.config, target).await.is_ok()
                    || !special.target(ctx.config, target).is_ok()
                    || !guard.target(ctx.config, target).is_ok()
                {
                    continue;
                }
                let rules = ctx.rules.get_rules_for_apply_region(&region);
                let factory = PeerOperatorFactory::new(
                    NAME,
                    BuilderContext {
                        stores: ctx.cluster.stores(),
                        config: ctx.config,
                        rules: &rules,
                        now_unix_nanos: ctx.now_unix_nanos,
                    },
                    &region,
                    ctx.supports_conf_change_v2,
                );
                let old = region
                    .get_store_peer(source.id())
                    .expect("source role index");
                let peer = Some(Peer {
                    store_id: target.id(),
                    role: old.role,
                    ..Default::default()
                });
                let op = if job.rule == Rule::LeaderScatter && existing {
                    factory
                        .create_transfer_leader(
                            target.id(),
                            vec![],
                            OpKind::RANGE,
                            &mut *ctx.allocate,
                        )
                        .await
                } else if region.leader().is_some_and(|p| p.store_id == source.id()) {
                    factory
                        .create_replace_leader_peer(
                            source.id(),
                            peer,
                            target.id(),
                            OpKind::RANGE,
                            &mut *ctx.allocate,
                        )
                        .await
                } else {
                    factory
                        .create_move_peer(source.id(), peer, OpKind::RANGE, &mut *ctx.allocate)
                        .await
                };
                match op {
                    Ok(op) => {
                        for (key, value) in [
                            ("sourceScore", self.scores[&source.id()]),
                            ("targetScore", self.scores[&target.id()]),
                            ("sourceExpectScore", self.expected[&source.id()]),
                            ("targetExpectScore", self.expected[&target.id()]),
                        ] {
                            op.additional_info().set(key, format!("{value:.2}"));
                        }
                        out.operators.push(Arc::new(op));
                    }
                    Err(e) => out.errors.push(e),
                }
                return out;
            }
        }
        out
    }
}
