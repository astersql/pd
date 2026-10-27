// Copyright 2026 AsterSQL.
//! Collect from the host's current root tree, never from the queued event snapshot.
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{RegionAnomalyDurations, RegionStatistics};

/// Cancellation after resolving the current region; statistics remain untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectCancelled;
impl std::fmt::Display for CollectCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("region collection cancelled")
    }
}
impl std::error::Error for CollectCancelled {}

/// Resolve the event ID again before observing real region and peer-store state.
/// The host serializes root mutations and statistics observation. None means the
/// basic cluster/region is absent, or placement statistics are not initialized.
#[allow(clippy::too_many_arguments)]
pub fn collect(
    basic: Option<&BasicCluster>,
    event: &RegionInfo,
    statistics: &mut RegionStatistics,
    config: &impl CheckerConfigProvider,
    rules: &RuleManager,
    now_unix_seconds: i64,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<RegionAnomalyDurations>, CollectCancelled> {
    let Some(basic) = basic else {
        return Ok(None);
    };
    let Some(region) = basic.regions.get_region(event.meta().id) else {
        return Ok(None);
    };
    if cancelled() {
        return Err(CollectCancelled);
    }
    let stores: Vec<_> = basic
        .stores
        .get_region_stores(&region)
        .iter()
        .map(|s| s.as_ref().clone())
        .collect();
    Ok(statistics.observe_with_placement(&region, &stores, config, rules, now_unix_seconds))
}
