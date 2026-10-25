// Copyright 2026 AsterSQL.
//! Affinity merge selection over canonical group, placement and operator state.
use crate::{AffinityChecker, AffinityContext, affinity_peer::best_location, check_peer_store};
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_affinity::{group::GroupState, manager::Manager};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::is_region_healthy;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, RangeOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use std::sync::Arc;

impl AffinityChecker {
    /// Initialize startup gating with host Unix nanoseconds. Default starts at zero.
    pub fn new(now_unix_nanos: i128) -> Self {
        let mut checker = Self::default();
        checker.start_time = now_unix_nanos;
        checker
    }
    /// Complete Go Check result: either one repair, two merge operators or empty.
    /// The host obtains adjacent immutable snapshots from its canonical region tree.
    /// Factory failures are typed diagnostics; successful completion must call
    /// record_op_success, rather than caching on creation or admission.
    #[allow(clippy::too_many_arguments)]
    pub async fn check_with_neighbors<C, F>(
        &self,
        ctx: &AffinityContext<'_, C>,
        rules: &mut RuleManager,
        manager: &mut Manager,
        region: &Arc<RegionInfo>,
        prev: Option<&Arc<RegionInfo>>,
        next: Option<&Arc<RegionInfo>>,
        allocate: &mut F,
    ) -> Result<Vec<Operator>, BuilderError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let Some((group, affinity)) = self.prepare_group(ctx, rules, manager, region).await else {
            return Ok(vec![]);
        };
        if !affinity {
            return self
                .create_affinity_operator(ctx, rules, region, &group, allocate)
                .await
                .map(|op| op.into_iter().collect());
        }
        let replica = &ctx.rules.replica;
        let now = replica.now_unix_nanos;
        let seconds = now.div_euclid(1_000_000_000) as u64;
        let conf = replica.config;
        let max_size = conf.get_max_affinity_merge_region_size() as i64;
        let max_keys = max_size.wrapping_mul(10000);
        if now < self.start_time.saturating_add(60_000_000_000)
            || self.recently_merged(region.meta().id, seconds)
            || max_size == 0
            || conf.get_max_merge_region_size() == 0
            || conf.get_merge_schedule_limit() == 0
            || !region.need_merge(max_size, max_keys)
        {
            return Ok(vec![]);
        }
        let mut target = None;
        if let Some(next) = next {
            if affinity_target(ctx, rules, manager, region, next, &group, seconds).await {
                target = Some(next);
            }
        }
        if !conf.is_one_way_merge_enabled() {
            if let Some(prev) = prev {
                if affinity_target(ctx, rules, manager, region, prev, &group, seconds).await
                    && target
                        .is_none_or(|t| prev.stats().approximate_size < t.stats().approximate_size)
                {
                    target = Some(prev);
                }
            }
        }
        let Some(target) = target else {
            return Ok(vec![]);
        };
        // Selection precedes cache and total-limit checks: no fallback afterwards.
        if self.recently_merged(target.meta().id, seconds)
            || region
                .stats()
                .approximate_size
                .wrapping_add(target.stats().approximate_size)
                > max_size
            || region
                .stats()
                .approximate_keys
                .wrapping_add(target.stats().approximate_keys)
                > max_keys
        {
            return Ok(vec![]);
        }
        let apply = rules.get_rules_for_apply_region(region);
        RangeOperatorFactory::new(
            "affinity-merge-region",
            BuilderContext {
                stores: replica.stores,
                config: conf,
                rules: &apply,
                now_unix_nanos: now,
            },
            region,
            replica.supports_conf_change_v2,
        )
        .create_merge(target, OpKind::AFFINITY, allocate)
        .await
        .map(Vec::from)
    }
}
async fn affinity_target<C: CheckerConfigProvider>(
    ctx: &AffinityContext<'_, C>,
    rules: &mut RuleManager,
    manager: &Manager,
    region: &RegionInfo,
    adjacent: &Arc<RegionInfo>,
    group: &GroupState,
    now: u64,
) -> bool {
    let (target, affinity) =
        manager.get_region_affinity_group_state(ctx.labels, Some(adjacent), now);
    if !affinity || target.is_none_or(|g| g.group.id != group.group.id) {
        return false;
    }
    let a = &region.meta().key_range;
    let b = &adjacent.meta().key_range;
    let (start, end) = if !a.end_key.is_empty() && a.end_key == b.start_key {
        (&a.start_key, &b.end_key)
    } else if !b.end_key.is_empty() && b.end_key == a.start_key {
        (&b.start_key, &a.end_key)
    } else {
        return false;
    };
    // Affinity deliberately ignores table identity and merge_option=deny.
    if !rules.get_split_keys(start, end).is_empty()
        || !ctx.labels.index().get_split_keys(start, end).is_empty()
        || !check_peer_store(ctx.rules.replica.stores, region, adjacent)
        || !is_region_healthy(adjacent)
    {
        return false;
    }
    best_location(ctx, rules, adjacent, true).await
}
