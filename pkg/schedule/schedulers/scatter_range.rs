// Copyright 2026 AsterSQL.
//! Range-local leader-first balancing using the canonical balancing engines.
use crate::{
    BalanceLeaderScheduler, BalanceRegionScheduler, LeaderBalanceContext, RangeCluster,
    RegionBalanceContext, RegionBalanceResult, SchedulerError, decode_config, encode_config,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_operator::OpKind;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScatterRangeConfig {
    #[serde(rename = "range-name")]
    pub range_name: String,
    #[serde(rename = "start-key")]
    pub start_key: String,
    #[serde(rename = "end-key")]
    pub end_key: String,
}
#[derive(Debug, thiserror::Error)]
pub enum ScatterRangeError {
    #[error("ranges and name require exactly three arguments")]
    Arguments,
    #[error("cannot change the range name, please delete this schedule")]
    Rename,
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
}
impl ScatterRangeConfig {
    /// Go's config builder takes name/start/end verbatim, including empty or reversed keys.
    pub fn from_args(args: &[&str]) -> Result<Self, ScatterRangeError> {
        if args.len() != 3 {
            return Err(ScatterRangeError::Arguments);
        }
        Ok(Self {
            range_name: args[0].into(),
            start_key: args[1].into(),
            end_key: args[2].into(),
        })
    }
}
/// Owns retry state for both canonical engines. Storage and operator submission belong to the host.
pub struct ScatterRangeScheduler {
    config: ScatterRangeConfig,
    name: String,
    leader: BalanceLeaderScheduler,
    region: BalanceRegionScheduler,
}
impl ScatterRangeScheduler {
    pub fn new(config: ScatterRangeConfig) -> Self {
        let mut leader = BalanceLeaderScheduler::default();
        leader.name = "scatter-range-leader".into();
        let mut region = BalanceRegionScheduler::default();
        region.name = "scatter-range-region".into();
        Self {
            name: format!("scatter-range-scheduler-{}", config.range_name),
            config,
            leader,
            region,
        }
    }
    pub fn config(&self) -> &ScatterRangeConfig {
        &self.config
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub fn allow_balance_leader(count: u64, limit: u64) -> bool {
        count < limit
    }
    pub fn allow_balance_region(count: u64, limit: u64) -> bool {
        count < limit
    }
    pub fn is_schedule_allowed(count: u64, leader_limit: u64, region_limit: u64) -> bool {
        Self::allow_balance_leader(count, leader_limit)
            || Self::allow_balance_region(count, region_limit)
    }
    pub async fn persist(
        &self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SchedulerError> {
        storage
            .save_scheduler_config(paths, &self.name, &self.encode_config()?)
            .await?;
        Ok(())
    }
    pub async fn reload(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SchedulerError> {
        let config = decode_config(&storage.load_scheduler_config(paths, &self.name).await?)?;
        self.config = config;
        Ok(())
    }
    /// HTTP adapters supply string fields only. Go publishes keys before persistence;
    /// failed writes retain new local keys but leave the durable snapshot untouched.
    pub async fn update(
        &mut self,
        name: Option<&str>,
        start: Option<&str>,
        end: Option<&str>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ScatterRangeError> {
        if name.is_some_and(|name| name != self.config.range_name) {
            return Err(ScatterRangeError::Rename);
        }
        if let Some(start) = start {
            self.config.start_key = start.into();
        }
        if let Some(end) = end {
            self.config.end_key = end.into();
        }
        self.persist(storage, paths).await?;
        Ok(())
    }
    /// `range_count` counts OpRange, not leader/region operations. dry-run is intentionally
    /// absent: Go always calls both child engines with false and discards their plans.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut RegionBalanceContext<'_, C, P>,
        range_count: u64,
    ) -> RegionBalanceResult
    where
        P::Version: Clone,
    {
        let view = RangeCluster::new(
            ctx.cluster,
            self.config.start_key.as_bytes(),
            self.config.end_key.as_bytes(),
        );
        let mut local = ctx.config.snapshot().clone();
        local.schedule.tolerant_size_ratio = 2.0;
        let mut result = RegionBalanceResult::default();
        if Self::allow_balance_leader(range_count, ctx.config.get_leader_schedule_limit()) {
            let got = self
                .leader
                .schedule(
                    &mut LeaderBalanceContext {
                        cluster: &view,
                        config: &local,
                        rules: ctx.rules,
                        affinity: AffinityFilter {
                            manager: ctx.affinity.manager,
                            labels: ctx.affinity.labels,
                            now: ctx.affinity.now,
                        },
                        statistics: ctx.statistics,
                        hot_degree: ctx.hot_degree,
                        key_ranges: ctx.key_ranges,
                        influence: ctx.influence,
                        now_unix_nanos: ctx.now_unix_nanos,
                        random_index: ctx.random_index,
                        shuffle: ctx.shuffle,
                    },
                    false,
                )
                .await;
            result.operators = got.operators;
            result.errors = got.errors;
            if !result.operators.is_empty() {
                Self::decorate(
                    &mut result,
                    &format!("scatter-range-leader-{}", self.config.range_name),
                );
                return result;
            }
        }
        if Self::allow_balance_region(range_count, ctx.config.get_region_schedule_limit()) {
            let got = self
                .region
                .schedule(
                    &mut RegionBalanceContext {
                        cluster: &view,
                        config: &local,
                        rules: ctx.rules,
                        affinity: AffinityFilter {
                            manager: ctx.affinity.manager,
                            labels: ctx.affinity.labels,
                            now: ctx.affinity.now,
                        },
                        statistics: ctx.statistics,
                        hot_degree: ctx.hot_degree,
                        key_ranges: ctx.key_ranges,
                        influence: ctx.influence,
                        now_unix_nanos: ctx.now_unix_nanos,
                        random_index: ctx.random_index,
                        shuffle: ctx.shuffle,
                        allocate: ctx.allocate,
                        supports_conf_change_v2: ctx.supports_conf_change_v2,
                        allow_empty_regions: true,
                    },
                    false,
                )
                .await;
            result.operators = got.operators;
            result.errors.extend(got.errors);
            if !result.operators.is_empty() {
                Self::decorate(
                    &mut result,
                    &format!("scatter-range-region-{}", self.config.range_name),
                );
            }
        }
        result
    }
    fn decorate(result: &mut RegionBalanceResult, desc: &str) {
        // Canonical engines exclusively own their new operations until returning them.
        Arc::get_mut(&mut result.operators[0])
            .expect("unpublished operation")
            .decorate(desc.into(), OpKind::RANGE);
    }
}
