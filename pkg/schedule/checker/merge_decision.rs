// Copyright 2026 AsterSQL.
use crate::{PauseController, merge_target_allowed};
use astersql_pd_core::{RegionInfo, StoresInfo};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_filter::{
    RegionReplicationContext, is_region_healthy, is_region_replicated,
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, RangeOperatorFactory,
};
use std::collections::{BTreeMap, BTreeSet};

/// Host snapshots for one merge evaluation. Hot IDs come from the canonical
/// statistics snapshot; no heat thresholds are reimplemented here.
pub struct MergeCheckContext<'a, C: CheckerConfigProvider> {
    pub replication: &'a RegionReplicationContext<'a, C>,
    pub stores: &'a StoresInfo,
    pub labels: Option<&'a RegionLabeler>,
    pub hot_regions: &'a BTreeSet<u64>,
    pub supports_conf_change_v2: bool,
}
/// Startup and split cooldown state. The host supplies Unix nanoseconds and
/// serializes evaluations, without a private runtime or background GC task.
pub struct MergeChecker {
    pub pause: PauseController,
    start_time: i128,
    interval: i64,
    split_cache: BTreeMap<u64, i128>,
}
impl MergeChecker {
    /// Create a checker at a host timestamp; interval is signed nanoseconds.
    pub fn new(now: i128, interval: i64) -> Self {
        Self {
            pause: PauseController::default(),
            start_time: now,
            interval,
            split_cache: BTreeMap::new(),
        }
    }
    /// Record each split with the configuration at the event's timestamp.
    pub fn record_region_split(&mut self, ids: &[u64], now: i128, interval: i64) {
        for &id in ids {
            self.split_cache
                .insert(id, now.saturating_add(interval as i128));
        }
    }
    /// Evaluate one immutable source/adjacency snapshot. Factory failures remain
    /// typed errors; gating returns None and never allocates peer IDs.
    pub async fn check<C, F>(
        &mut self,
        context: MergeCheckContext<'_, C>,
        region: &RegionInfo,
        prev: Option<&RegionInfo>,
        next: Option<&RegionInfo>,
        allocate: F,
    ) -> Result<Option<[Operator; 2]>, BuilderError>
    where
        C: CheckerConfigProvider,
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let rep = context.replication;
        let now = rep.now;
        let conf = rep.conf;
        if self
            .pause
            .is_paused_at(now.div_euclid(1_000_000_000) as i64)
        {
            return Ok(None);
        }
        let interval = conf.get_split_merge_interval().as_nanos();
        // Go UpdateTTL resets all retained entries relative to the current time,
        // even while startup gating still rejects the source (issue #8405).
        if interval != self.interval {
            for deadline in self.split_cache.values_mut() {
                *deadline = now.saturating_add(interval as i128);
            }
            self.interval = interval;
        }
        self.split_cache.retain(|_, deadline| *deadline >= now);
        if now < self.start_time.saturating_add(interval as i128)
            || self.split_cache.contains_key(&region.meta().id)
            || region.leader().is_none()
            || !region.need_merge(
                conf.get_max_merge_region_size() as i64,
                conf.get_max_merge_region_keys() as i64,
            )
            || !is_region_healthy(region)
            || !is_region_replicated(rep, region)
            || context.hot_regions.contains(&region.meta().id)
        {
            return Ok(None);
        }
        let allowed = |candidate: Option<&RegionInfo>| {
            candidate.is_some_and(|adjacent| {
                !self.split_cache.contains_key(&adjacent.meta().id)
                    && !context.hot_regions.contains(&adjacent.meta().id)
                    && merge_target_allowed(
                        rep,
                        context.labels,
                        context.stores,
                        region,
                        Some(adjacent),
                    )
            })
        };
        let mut target = if allowed(next) { next } else { None };
        if !conf.is_one_way_merge_enabled()
            && allowed(prev)
            && target.is_none_or(|target| {
                prev.unwrap().stats().approximate_size < target.stats().approximate_size
            })
        {
            target = prev;
        }
        let Some(target) = target else {
            return Ok(None);
        };
        // Mirror Go's uint64 -> float64 -> int64 threshold conversion.
        let scaled = conf.get_region_max_size() as f64 * 4.0;
        let cap = if scaled >= 9_223_372_036_854_775_808.0 {
            i64::MIN
        } else {
            scaled as i64
        }
        .max(500);
        if target.stats().approximate_size > cap
            || conf
                .check_region_size(
                    target
                        .stats()
                        .approximate_size
                        .wrapping_add(region.stats().approximate_size) as u64,
                    conf.get_max_merge_region_size(),
                )
                .is_err()
            || conf
                .check_region_keys(
                    target
                        .stats()
                        .approximate_keys
                        .wrapping_add(region.stats().approximate_keys) as u64,
                    conf.get_max_merge_region_keys(),
                )
                .is_err()
        {
            return Ok(None);
        }
        let rules = if conf.is_placement_rules_enabled() {
            rep.manager.borrow().get_rules_for_apply_region(region)
        } else {
            vec![]
        };
        RangeOperatorFactory::new(
            "merge-region",
            BuilderContext {
                stores: context.stores,
                config: conf,
                rules: &rules,
                now_unix_nanos: now,
            },
            region,
            context.supports_conf_change_v2,
        )
        .create_merge(target, OpKind::MERGE, allocate)
        .await
        .map(Some)
    }
}
