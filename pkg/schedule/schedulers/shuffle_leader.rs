// Copyright 2026 AsterSQL.
//! Random target-first administrative leader transfer over canonical follower indexes.
use crate::{
    EvictLeaderContext, EvictLeaderResult, KeyRangeDecodeError, SchedulerError, encode_config,
    get_key_ranges,
};
use astersql_pd_core::KeyRange;
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    Filter, RegionDownFilter, RegionPendingFilter, SpecialUseFilter, StoreStateFilter,
    select_one_region,
};
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};
use std::sync::Arc;
/// CLI defaults cover all keys; JSON preserves absent/null versus empty ranges.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ShuffleLeaderConfig {
    #[serde(default)]
    ranges: Option<Vec<astersql_pd_keyutil::KeyRange>>,
}
impl ShuffleLeaderConfig {
    pub fn new(ranges: Vec<KeyRange>) -> Self {
        Self {
            ranges: Some(
                ranges
                    .into_iter()
                    .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                    .collect(),
            ),
        }
    }
    pub fn from_args(args: &[&str]) -> Result<Self, KeyRangeDecodeError> {
        Ok(Self::new(get_key_ranges(args)?))
    }
    pub fn ranges(&self) -> Vec<KeyRange> {
        self.ranges
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|r| KeyRange {
                start_key: r.start_key.clone(),
                end_key: r.end_key.clone(),
            })
            .collect()
    }
}
/// Host owns admission, random draws and time. Each call attempts just one target.
pub struct ShuffleLeaderScheduler {
    config: ShuffleLeaderConfig,
}
impl ShuffleLeaderScheduler {
    pub fn new(config: ShuffleLeaderConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &ShuffleLeaderConfig {
        &self.config
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    /// Sort store IDs to normalize unspecified Go map order. Do not retry a target
    /// without healthy followers. Retain typed builder errors for host diagnostics.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
    ) -> EvictLeaderResult {
        let mut result = EvictLeaderResult::default();
        let state = StoreStateFilter {
            action_scope: "shuffle-leader-scheduler".into(),
            transfer_leader: true,
            operator_level: PriorityLevel::LOW,
            now_unix_nanos: ctx.now_unix_nanos,
            ..Default::default()
        };
        let special = SpecialUseFilter::new("shuffle-leader-scheduler", &[]);
        let mut candidates = vec![];
        for store in ctx.cluster.get_stores() {
            if state.target(ctx.config, &store).await.is_ok()
                && special.target(ctx.config, &store).is_ok()
            {
                candidates.push(store.id());
            }
        }
        candidates.sort_unstable();
        if candidates.is_empty() {
            return result;
        }
        let target = candidates[(ctx.random_index)(candidates.len())];
        let sampled = ctx.cluster.rand_follower_regions(
            target,
            &self.config.ranges(),
            &mut *ctx.random_index,
            &mut *ctx.shuffle,
        );
        let Some(region) =
            select_one_region(&sampled, None, &[&RegionPendingFilter, &RegionDownFilter])
        else {
            return result;
        };
        let rules = ctx.rules.get_rules_for_apply_range(
            &region.meta().key_range.start_key,
            &region.meta().key_range.end_key,
        );
        let factory = PeerOperatorFactory::new(
            "shuffle-leader-scheduler",
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
            .create_transfer_leader(target, vec![], OpKind::ADMIN, || {
                Err(BuilderError::Allocation(
                    "leader transfer must not allocate".into(),
                ))
            })
            .await
        {
            Ok(op) => {
                op.set_priority_level(PriorityLevel::LOW);
                result.operators.push(Arc::new(op));
            }
            Err(error) => result.errors.push(error),
        }
        result
    }
}
