// Copyright 2026 AsterSQL.
//! Administrative leader grants over canonical follower indexes and operator builders.
use astersql_pd_core::{KeyRange, StoreControlError};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{RegionDownFilter, RegionPendingFilter, select_one_region};
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, PeerOperatorFactory};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Per-store contained key ranges. Empty ranges select no followers.
#[derive(Clone, Debug, Default)]
pub struct GrantLeaderConfig {
    stores: BTreeMap<u64, Vec<KeyRange>>,
}
impl GrantLeaderConfig {
    pub fn new(stores: BTreeMap<u64, Vec<KeyRange>>) -> Self {
        Self { stores }
    }
    pub fn stores(&self) -> &BTreeMap<u64, Vec<KeyRange>> {
        &self.stores
    }
}
/// Shared canonical state with explicit random and clock boundaries.
pub type GrantLeaderContext<'a, C, P> = crate::EvictLeaderContext<'a, C, P>;
/// Builder failures are retained for diagnostics; Go logs and skips them.
pub type GrantLeaderResult = crate::EvictLeaderResult;
pub struct GrantLeaderScheduler {
    config: GrantLeaderConfig,
}
impl GrantLeaderScheduler {
    pub fn new(config: GrantLeaderConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &GrantLeaderConfig {
        &self.config
    }
    pub fn is_schedule_allowed(operator_count: u64, limit: u64) -> bool {
        operator_count < limit
    }
    /// Attempt every store, preserving successful pauses on error, as Go does.
    /// The owner must clean after every prepare attempt, including failures.
    pub fn prepare<C: BasicCluster>(&self, cluster: &C) -> Result<(), StoreControlError> {
        let mut result = Ok(());
        for id in self.config.stores.keys() {
            if let Err(error) = cluster.pause_leader_transfer(*id, Direction::OUT) {
                result = Err(error);
            }
        }
        result
    }
    pub fn clean<C: BasicCluster>(&self, cluster: &C) {
        for id in self.config.stores.keys() {
            cluster.resume_leader_transfer(*id, Direction::OUT);
        }
    }
    /// Select at most one healthy follower per target, independent of allowance
    /// checking (the coordinator owns the separate schedule-limit gate).
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut GrantLeaderContext<'_, C, P>,
    ) -> GrantLeaderResult {
        let mut result = GrantLeaderResult::default();
        for (store, ranges) in &self.config.stores {
            if ranges.is_empty() {
                continue;
            }
            let sampled = ctx.cluster.rand_follower_regions(
                *store,
                ranges,
                &mut *ctx.random_index,
                &mut *ctx.shuffle,
            );
            let Some(region) =
                select_one_region(&sampled, None, &[&RegionPendingFilter, &RegionDownFilter])
            else {
                continue;
            };
            let rules = ctx.rules.get_rules_for_apply_range(
                &region.meta().key_range.start_key,
                &region.meta().key_range.end_key,
            );
            let factory = PeerOperatorFactory::new(
                "grant-leader-scheduler",
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
                .create_force_transfer_leader(*store, OpKind::LEADER, || {
                    Err(BuilderError::Allocation(
                        "leader transfer must not allocate".into(),
                    ))
                })
                .await
            {
                Ok(op) => {
                    op.set_priority_level(PriorityLevel::HIGH);
                    result.operators.push(Arc::new(op));
                }
                Err(error) => result.errors.push(error),
            }
        }
        result
    }
}
