// Copyright 2026 AsterSQL.
//! Reject-label leader evacuation using the canonical follower/state and operator pipeline.
use crate::{
    EvictLeaderContext, EvictLeaderResult, KeyRangeDecodeError, SchedulerError, encode_config,
    get_key_ranges,
};
use astersql_pd_core::KeyRange;
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::{REJECT_LEADER, SchedulerConfigProvider};
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{StoreStateFilter, select_one_region};
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};
use std::{collections::HashSet, sync::Arc};
/// CLI defaults cover all keys; JSON preserves absent/null versus empty ranges.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct LabelConfig {
    #[serde(default)]
    ranges: Option<Vec<astersql_pd_keyutil::KeyRange>>,
}
impl LabelConfig {
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
/// Emits at most one MEDIUM leader operator. Admission count is checked by the host.
pub struct LabelScheduler {
    config: LabelConfig,
}
impl LabelScheduler {
    pub fn new(config: LabelConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &LabelConfig {
        &self.config
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&self.config)
    }
    pub fn is_schedule_allowed(count: u64, limit: u64) -> bool {
        count < limit
    }
    /// Store/map order in Go is unspecified. Sort IDs and accept host-owned random input.
    /// A builder failure ends the attempt, retaining its typed error for diagnostics.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
    ) -> EvictLeaderResult {
        let mut result = EvictLeaderResult::default();
        let mut sources: Vec<_> = ctx
            .cluster
            .get_stores()
            .into_iter()
            .filter(|s| ctx.config.check_label_property(REJECT_LEADER, s.labels()))
            .collect();
        sources.sort_by_key(|s| s.id());
        for source in sources {
            let sampled = ctx.cluster.rand_leader_regions(
                source.id(),
                &self.config.ranges(),
                &mut *ctx.random_index,
                &mut *ctx.shuffle,
            );
            let Some(region) = select_one_region(&sampled, None, &[]) else {
                continue;
            };
            let excluded: HashSet<_> = region
                .down_peers()
                .iter()
                .map(|p| p.peer.store_id)
                .chain(region.pending_peers().iter().map(|p| p.store_id))
                .collect();
            let state = StoreStateFilter {
                action_scope: "label-scheduler".into(),
                transfer_leader: true,
                operator_level: PriorityLevel::MEDIUM,
                now_unix_nanos: ctx.now_unix_nanos,
                ..Default::default()
            };
            let mut candidates = Vec::new();
            for target in ctx.cluster.get_follower_stores(&region) {
                if !excluded.contains(&target.id())
                    && state.target(ctx.config, &target).await.is_ok()
                {
                    candidates.push(target.id());
                }
            }
            candidates.sort_unstable();
            if candidates.is_empty() {
                continue;
            }
            let target = candidates[(ctx.random_index)(candidates.len())];
            let rules = ctx.rules.get_rules_for_apply_range(
                &region.meta().key_range.start_key,
                &region.meta().key_range.end_key,
            );
            let factory = PeerOperatorFactory::new(
                "label-reject-leader",
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
                .create_transfer_leader(target, vec![], OpKind::LEADER, || {
                    Err(BuilderError::Allocation(
                        "leader transfer must not allocate".into(),
                    ))
                })
                .await
            {
                Ok(op) => result.operators.push(Arc::new(op)),
                Err(error) => result.errors.push(error),
            }
            return result;
        }
        result
    }
}
