// Copyright 2026 AsterSQL.
//! Bucket report admission over the canonical region cache.
use astersql_pd_core::{BasicCluster, RegionBucketStats, RegionBuckets};
use astersql_pd_statistics_buckets::{BucketReport, BucketWorkerClient};
use std::sync::Arc;

/// A report addressed a region absent from the cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BucketRegionNotFound(pub u64);
impl std::fmt::Display for BucketRegionNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "region {} not found", self.0)
    }
}
impl std::error::Error for BucketRegionNotFound {}

pub(super) fn retry_bucket_update(mut update: impl FnMut() -> bool) -> bool {
    for _ in 0..3 {
        if update() {
            return true;
        }
    }
    false
}

/// Accept a report and submit admitted work to the optional local bucket worker.
/// The caller holds exclusive cluster access through cache commit and submission.
/// `true` includes equal/stale versions that core accepts without replacement;
/// `false` denotes an ignored Removed leader or exhausted CAS retries. Queue
/// submission is best effort and cannot undo the accepted report. Passing no
/// worker leaves local hot-bucket processing to the host's service topology.
pub fn handle_bucket_heartbeat(
    cluster: &mut BasicCluster,
    worker: Option<&BucketWorkerClient>,
    report: BucketReport,
) -> Result<bool, BucketRegionNotFound> {
    let origin = cluster
        .regions
        .get_region(report.region_id)
        .ok_or(BucketRegionNotFound(report.region_id))?;
    if cluster
        .get_leader_store_by_region_id(report.region_id)
        .is_some_and(|store| store.is_removed())
    {
        return Ok(false);
    }
    let buckets = Arc::new(RegionBuckets {
        region_id: report.region_id,
        version: report.version,
        keys: report.keys.clone(),
        period_in_ms: report.period_in_ms,
        stats: Some(RegionBucketStats {
            read_bytes: report.flows.read_bytes.clone(),
            read_keys: report.flows.read_keys.clone(),
            read_qps: report.flows.read_qps.clone(),
            write_bytes: report.flows.write_bytes.clone(),
            write_keys: report.flows.write_keys.clone(),
            write_qps: report.flows.write_qps.clone(),
        }),
    });
    let mut next = origin.as_ref().clone();
    let applied =
        retry_bucket_update(|| next.compare_and_set_report_buckets(Some(buckets.clone())));
    if applied {
        // Equal/stale reports succeed without changing the snapshot. Publish only
        // replacements, updating both root and role indexes under the host lock.
        if next
            .report_buckets()
            .zip(origin.report_buckets())
            .is_none_or(|(next, old)| !Arc::ptr_eq(next, old))
        {
            cluster.regions.put_region(Arc::new(next));
        }
        if let Some(worker) = worker {
            // Queue admission is best effort, as in Go CheckAsync: a full/closed
            // worker does not roll back accepted region metadata.
            let _ = worker.check_async(report);
        }
    }
    Ok(applied)
}
