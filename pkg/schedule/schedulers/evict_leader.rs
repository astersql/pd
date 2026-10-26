// Copyright 2026 AsterSQL.
//! Administrative eviction over configured stores and their contained key ranges.
use crate::InvalidLeaderBatch;
use astersql_pd_core::{KeyRange, StoreControlError};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_filter::{
    RegionDownFilter, RegionPendingFilter, StoreStateFilter, select_one_region,
};
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

/// Empty store ranges disable eviction for that store; an empty key pair covers all keys.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "EvictConfigJson", into = "EvictConfigJson")]
pub struct EvictLeaderConfig {
    stores: BTreeMap<u64, Vec<KeyRange>>,
    batch: usize,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct EvictConfigJson {
    #[serde(rename = "store-id-ranges")]
    stores: BTreeMap<u64, Vec<astersql_pd_keyutil::KeyRange>>,
    #[serde(default)]
    batch: usize,
}
impl TryFrom<EvictConfigJson> for EvictLeaderConfig {
    type Error = InvalidLeaderBatch;
    fn try_from(value: EvictConfigJson) -> Result<Self, Self::Error> {
        Self::new(
            value
                .stores
                .into_iter()
                .map(|(s, r)| {
                    (
                        s,
                        r.into_iter()
                            .map(|r| KeyRange {
                                start_key: r.start_key,
                                end_key: r.end_key,
                            })
                            .collect(),
                    )
                })
                .collect(),
            if value.batch == 0 { 3 } else { value.batch },
        )
    }
}
impl From<EvictLeaderConfig> for EvictConfigJson {
    fn from(value: EvictLeaderConfig) -> Self {
        Self {
            batch: value.batch,
            stores: value
                .stores
                .into_iter()
                .map(|(s, r)| {
                    (
                        s,
                        r.into_iter()
                            .map(|r| astersql_pd_keyutil::KeyRange::new(r.start_key, r.end_key))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}
impl Default for EvictLeaderConfig {
    fn default() -> Self {
        Self {
            stores: BTreeMap::new(),
            batch: 3,
        }
    }
}
impl EvictLeaderConfig {
    pub fn new(
        stores: BTreeMap<u64, Vec<KeyRange>>,
        batch: usize,
    ) -> Result<Self, InvalidLeaderBatch> {
        if !(1..=100).contains(&batch) {
            return Err(InvalidLeaderBatch(batch));
        }
        Ok(Self { stores, batch })
    }
    pub fn stores(&self) -> &BTreeMap<u64, Vec<KeyRange>> {
        &self.stores
    }
    pub fn batch(&self) -> usize {
        self.batch
    }
}
/// Borrowed canonical state and explicit random/clock inputs; no private runtime or cache.
pub struct EvictLeaderContext<'a, C, P> {
    pub cluster: &'a C,
    pub config: &'a P,
    pub rules: &'a mut RuleManager,
    pub now_unix_nanos: i128,
    pub random_index: &'a mut dyn FnMut(usize) -> usize,
    pub shuffle: &'a mut dyn FnMut(&mut [usize]),
}
#[derive(Default)]
pub struct EvictLeaderResult {
    pub operators: Vec<Arc<Operator>>,
    pub errors: Vec<BuilderError>,
}
pub struct EvictLeaderScheduler {
    pub(super) config: EvictLeaderConfig,
}
impl EvictLeaderScheduler {
    pub fn new(config: EvictLeaderConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &EvictLeaderConfig {
        &self.config
    }
    pub fn is_schedule_allowed(operator_count: u64, limit: u64) -> bool {
        operator_count < limit
    }
    /// Match Go's attempt-all behavior: partial pauses remain, and the last error is returned.
    /// The coordinator must pair every prepare attempt with clean, including a failed attempt.
    pub fn prepare<C: BasicCluster>(&self, cluster: &C) -> Result<(), StoreControlError> {
        let mut result = Ok(());
        for id in self.config.stores.keys() {
            if let Err(error) = cluster.pause_leader_transfer(*id, Direction::IN) {
                result = Err(error);
            }
        }
        result
    }
    pub fn clean<C: BasicCluster>(&self, cluster: &C) {
        for id in self.config.stores.keys() {
            cluster.resume_leader_transfer(*id, Direction::IN);
        }
    }
    /// Each round selects at most one region per configured store. The strict `>`
    /// batch break and whole-round append intentionally retain Go's multi-store overshoot.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
    ) -> EvictLeaderResult {
        self.schedule_with_description(ctx, "evict-leader-scheduler")
            .await
    }
    pub(super) async fn schedule_with_description<C: BasicCluster, P: SchedulerConfigProvider>(
        &self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
        name: &str,
    ) -> EvictLeaderResult {
        let mut result = EvictLeaderResult::default();
        let mut used = HashSet::new();
        for _ in 0..self.config.batch {
            let mut once = Vec::new();
            for (store, ranges) in &self.config.stores {
                if ranges.is_empty() {
                    continue;
                }
                let sampled = ctx.cluster.rand_leader_regions(
                    *store,
                    ranges,
                    &mut *ctx.random_index,
                    &mut *ctx.shuffle,
                );
                let healthy =
                    select_one_region(&sampled, None, &[&RegionPendingFilter, &RegionDownFilter]);
                let unhealthy = healthy.is_none();
                let region = if let Some(r) = healthy {
                    Some(r)
                } else {
                    let sampled = ctx.cluster.rand_leader_regions(
                        *store,
                        ranges,
                        &mut *ctx.random_index,
                        &mut *ctx.shuffle,
                    );
                    select_one_region(&sampled, None, &[])
                };
                let Some(region) = region else {
                    continue;
                };
                let excluded: HashSet<_> = if unhealthy {
                    region
                        .pending_peers()
                        .iter()
                        .map(|p| p.store_id)
                        .chain(region.down_peers().iter().map(|p| p.peer.store_id))
                        .collect()
                } else {
                    HashSet::new()
                };
                let state = StoreStateFilter {
                    action_scope: name.into(),
                    transfer_leader: true,
                    operator_level: PriorityLevel::URGENT,
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
                // Go map ordering and random choices are unspecified; a stable candidate order
                // permits repeatable caller-owned random inputs without changing the target set.
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
                    name,
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
                    .create_transfer_leader(target, candidates, OpKind::LEADER, || {
                        Err(BuilderError::Allocation(
                            "leader transfer must not allocate".into(),
                        ))
                    })
                    .await
                {
                    Ok(op) => {
                        op.set_priority_level(PriorityLevel::URGENT);
                        once.push(Arc::new(op));
                    }
                    Err(error) => result.errors.push(error),
                }
            }
            if once.is_empty() {
                break;
            }
            for op in once {
                if used.insert(op.region_id()) {
                    result.operators.push(op);
                }
            }
            if result.operators.len() > self.config.batch {
                break;
            }
        }
        result
    }
}
