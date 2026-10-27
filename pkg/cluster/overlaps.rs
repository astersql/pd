// Copyright 2026 AsterSQL.
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_affinity::manager::Manager;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{LabelStatistics, RegionStatistics};
use std::sync::Arc;

/// Cancellation preserves all effects for the completed prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlapsCancelled {
    pub processed: usize,
}

/// Shared statistics boundary for heartbeat and scheduling compositions.
pub fn clear_overlap_statistics(
    id: u64,
    stats: Option<&mut RegionStatistics>,
    labels: &mut LabelStatistics,
) {
    if let Some(stats) = stats {
        stats.clear_defunct_region(id);
    }
    labels.mark_defunct_region(id);
}

/// Invalidate evicted IDs, checking host cancellation before each whole region.
pub fn handle_overlaps(
    overlaps: &[Arc<RegionInfo>],
    mut stats: Option<&mut RegionStatistics>,
    labels: &mut LabelStatistics,
    rules: &mut RuleManager,
    mut affinity: Option<&mut Manager>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<usize, OverlapsCancelled> {
    for (processed, region) in overlaps.iter().enumerate() {
        if cancelled() {
            return Err(OverlapsCancelled { processed });
        }
        let id = region.meta().id;
        clear_overlap_statistics(id, stats.as_deref_mut(), labels);
        if let Some(manager) = affinity.as_deref_mut() {
            manager.invalid_cache(id);
        }
        rules.invalid_cache(id);
    }
    Ok(overlaps.len())
}

impl std::fmt::Display for OverlapsCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "overlap cleanup cancelled after {} regions",
            self.processed
        )
    }
}
impl std::error::Error for OverlapsCancelled {}
