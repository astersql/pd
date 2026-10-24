// Copyright 2026 AsterSQL.
//! Region heartbeat admission over the canonical core indexes.
use astersql_pd_core::{BasicCluster, RegionGuide, RegionInfo, StaleRegionError, region_guide};
use std::sync::Arc;

/// A validated, inherited report awaiting a commit against current state.
///
/// The host may release its cluster read lock between preparation and commit.
/// Cache-changing reports are rechecked under its exclusive commit lock. Guide
/// decisions use the preparation snapshot, as in Go; equal-epoch concurrent
/// reports can overwrite statistics in commit order, without a stronger total order.
pub struct PreparedRegionHeartbeat {
    region: Arc<RegionInfo>,
    guide: RegionGuide,
}
/// Accepted report and follow-up decisions for the host's side-effect pipeline.
#[derive(Debug)]
pub struct RegionHeartbeatResult {
    /// Inherited incoming report, including reports skipped by the guide.
    pub region: Arc<RegionInfo>,
    /// Storage/sync/retention decisions; side effects are owned by the host.
    pub guide: RegionGuide,
    /// Regions evicted by this root commit, excluding the same-ID origin.
    pub overlaps: Vec<Arc<RegionInfo>>,
}
/// Prepare a report without mutating cluster state.
pub fn prepare_region_heartbeat(
    cluster: &BasicCluster,
    mut region: RegionInfo,
    bucket_enabled: bool,
) -> Result<PreparedRegionHeartbeat, StaleRegionError> {
    let (origin, _, validation) = cluster.regions.pre_check_put_region(&region);
    validation?;
    region.inherit(origin.as_deref(), bucket_enabled);
    let guide = region_guide(&region, origin.as_deref());
    Ok(PreparedRegionHeartbeat {
        region: Arc::new(region),
        guide,
    })
}
impl PreparedRegionHeartbeat {
    /// Commit cache-changing reports after rechecking current state.
    /// Unchanged reports succeed without a second root check or write, matching
    /// the Go fast path; term-only changes do not force a cache update.
    pub fn commit(
        self,
        cluster: &mut BasicCluster,
    ) -> Result<RegionHeartbeatResult, StaleRegionError> {
        let result = self.commit_root(cluster)?;
        // Refresh from root so a delayed report cannot restore an evicted ID.
        cluster.regions.check_and_put_sub_tree(&result.region);
        Ok(result)
    }
    pub(crate) fn commit_root(
        self,
        cluster: &mut BasicCluster,
    ) -> Result<RegionHeartbeatResult, StaleRegionError> {
        let overlaps = if self.guide.save_cache {
            cluster
                .regions
                .check_and_put_root_tree(self.region.clone())?
        } else {
            vec![]
        };
        Ok(RegionHeartbeatResult {
            region: self.region,
            guide: self.guide,
            overlaps,
        })
    }
}
/// Prepare and commit while the caller holds exclusive cluster access.
pub fn handle_region_heartbeat(
    cluster: &mut BasicCluster,
    region: RegionInfo,
    bucket_enabled: bool,
) -> Result<RegionHeartbeatResult, StaleRegionError> {
    prepare_region_heartbeat(cluster, region, bucket_enabled)?.commit(cluster)
}
