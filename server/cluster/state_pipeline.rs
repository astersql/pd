// Copyright 2026 AsterSQL.
//! State-only composition over host-owned heartbeat, statistics and sync owners.
use crate::{
    BucketRegionNotFound, HeartbeatEffectsReceipt, HeartbeatEffectsState, HeartbeatRunners,
    StoreNotFound, handle_bucket_heartbeat, handle_store_heartbeat, submit_region_heartbeat,
};
use astersql_pd_core::{NodeState, RegionInfo, StaleRegionError, StoreRegionStats, StoreStats};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_statistics::{HotStat, StatisticsSnapshot, StatisticsSnapshotError};
use astersql_pd_statistics_buckets::BucketReport;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_syncer::Downstreams;
use std::sync::{Arc, Mutex};

/// Borrowed wiring. The host serializes admissions, owns runners/HotStat and joins
/// them on exit. Use the same Downstreams for submission and follower bindings.
/// No scheduler, transport, private runtime or second history is created here.
pub struct StatePipeline<'a, C> {
    pub state: &'a Arc<Mutex<HeartbeatEffectsState<C>>>,
    pub hot: &'a HotStat,
    pub runners: HeartbeatRunners<'a>,
    pub storage: Arc<StorageEndpoint>,
    pub paths: ClusterPaths,
    pub downstreams: Downstreams,
    pub bucket_enabled: bool,
}
/// Owned read view. Statistics FIFO barriers follow previously admitted reports.
/// Quiesce admissions and wait for effect receipts for a cross-cache checkpoint;
/// concurrent admissions do not promise a single atomic cluster-wide revision.
pub struct StateSnapshot {
    pub regions: Vec<Arc<RegionInfo>>,
    pub stores: Vec<(u64, StoreStats)>,
    pub store_regions: Vec<(u64, StoreRegionStats)>,
    pub labels: std::collections::BTreeMap<String, i64>,
    pub classifications: Vec<(u64, u16)>,
    pub statistics: StatisticsSnapshot,
}
impl<C: CheckerConfigProvider + Send + 'static> StatePipeline<'_, C> {
    pub fn store(
        &self,
        report: &StoreStats,
        now_unix_nanos: i64,
    ) -> Result<NodeState, StoreNotFound> {
        let mut state = self.state.lock().unwrap();
        handle_store_heartbeat(&mut state.cluster, Some(self.hot), report, now_unix_nanos)
    }
    pub fn bucket(&self, report: BucketReport) -> Result<bool, BucketRegionNotFound> {
        let mut state = self.state.lock().unwrap();
        handle_bucket_heartbeat(&mut state.cluster, Some(&self.hot.bucket_client()), report)
    }
    /// Acceptance is synchronous; the receipt reports independent effect failures.
    /// Unix seconds serve region collection; nanoseconds serve the hot caches.
    pub fn region(
        &self,
        region: RegionInfo,
        now_unix_seconds: i64,
        now_unix_nanos: i64,
    ) -> Result<HeartbeatEffectsReceipt, StaleRegionError> {
        let receipt = submit_region_heartbeat(
            self.state,
            region,
            self.bucket_enabled,
            &self.runners,
            Some(self.storage.clone()),
            self.paths,
            Some(self.downstreams.clone()),
            now_unix_seconds,
        )?;
        // Duplicate admissions still carry flow samples; stale reports never reach here.
        // Queue admission is best effort, matching the canonical Go hot-cache boundary.
        let _ = self
            .hot
            .hot_cache
            .check_region_flow_async(&receipt.heartbeat.region, now_unix_nanos);
        Ok(receipt)
    }
    pub async fn snapshot(&self) -> Result<StateSnapshot, StatisticsSnapshotError> {
        let statistics = self.hot.snapshot(-20, vec![]).await?;
        let state = self.state.lock().unwrap();
        let mut regions = state.cluster.regions.get_regions();
        regions.sort_by_key(|r| r.meta().id);
        let classifications = regions
            .iter()
            .filter_map(|r| {
                state
                    .region_stats
                    .as_ref()?
                    .bitmap(r.meta().id)
                    .map(|bits| (r.meta().id, bits))
            })
            .collect();
        let mut stores: Vec<_> = state
            .cluster
            .stores
            .get_stores()
            .into_iter()
            .map(|s| (s.id(), s.store_stats()))
            .collect();
        stores.sort_by_key(|s| s.0);
        let store_regions = stores
            .iter()
            .map(|(id, _)| (*id, state.cluster.regions.store_stats(*id)))
            .collect();
        let labels = state.label_stats.label_counter();
        Ok(StateSnapshot {
            regions,
            stores,
            store_regions,
            labels,
            classifications,
            statistics,
        })
    }
}
