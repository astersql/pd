// Copyright 2026 AsterSQL.
//! Apply overlap statistics after a successful core heartbeat commit.
use crate::{RegionHeartbeatResult, handle_region_heartbeat};
use astersql_pd_core::{BasicCluster, RegionInfo, StaleRegionError};
use astersql_pd_statistics::{LabelStatistics, RegionStatistics};

/// Commit a heartbeat, then clear evicted anomaly entries and defer label cleanup.
/// The host serializes this call with core and statistics mutations. Stale reports
/// leave both statistics indexes unchanged. Hot peers keep their normal cold/TTL
/// lifecycle, permitting inheritance; overlap alone does not remove their history.
/// Storage, placement and affinity cache invalidation remain host responsibilities.
pub fn handle_region_heartbeat_with_overlap_cleanup(
    cluster: &mut BasicCluster,
    region: RegionInfo,
    bucket_enabled: bool,
    mut region_stats: Option<&mut RegionStatistics>,
    label_stats: &mut LabelStatistics,
) -> Result<RegionHeartbeatResult, StaleRegionError> {
    let result = handle_region_heartbeat(cluster, region, bucket_enabled)?;
    for overlap in &result.overlaps {
        let id = overlap.meta().id;
        if let Some(stats) = region_stats.as_deref_mut() {
            stats.clear_defunct_region(id);
        }
        label_stats.mark_defunct_region(id);
    }
    Ok(result)
}
