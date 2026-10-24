// Copyright 2026 AsterSQL.
//! Follower response application over canonical region indexes and storage.
use crate::{HistoryBuffer, HistoryError, HistoryIndex};
use astersql_pd_core::{BasicCluster, Peer, RegionBucketMeta, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::Storage;
#[derive(Clone, Debug, Default)]
/// Raw sync flow counters, without heartbeat size conversion or filtering.
pub struct SyncRegionStats {
    pub written_bytes: u64,
    pub written_keys: u64,
    pub read_bytes: u64,
    pub read_keys: u64,
}
#[derive(Clone, Debug)]
/// An error frame; any present code, including zero, prevents application.
pub struct SyncResponseError {
    pub code: i32,
    pub message: String,
}
#[derive(Clone, Debug, Default)]
/// Owned response fields supplied by the protocol adapter.
pub struct SyncApplyResponse {
    pub error: Option<SyncResponseError>,
    pub start_index: HistoryIndex,
    pub regions: Vec<RegionMeta>,
    pub stats: Vec<SyncRegionStats>,
    pub buckets: Vec<Option<RegionBucketMeta>>,
    pub leaders: Vec<Option<Peer>>,
}
#[derive(Debug, Default, PartialEq, Eq)]
/// Application counts; storage failures preserve Go best-effort semantics.
pub struct ApplyOutcome {
    pub handled: bool,
    pub applied: usize,
    pub stale: usize,
    pub save_errors: usize,
    pub delete_errors: usize,
    /// Automatic history checkpoint failures; records remain assigned in memory.
    pub checkpoint_errors: usize,
}
/// The canonical history operations needed by a follower. Hosts may retain either
/// an in-memory ring or the existing automatic-checkpoint wrapper.
pub trait ApplyHistory {
    fn next_index(&self) -> HistoryIndex;
    fn reset_with_index(&mut self, index: HistoryIndex);
    /// Return false for a failed checkpoint after a successful in-memory record.
    fn record_sync(
        &mut self,
        region: std::sync::Arc<astersql_pd_core::RegionInfo>,
    ) -> impl std::future::Future<Output = Result<bool, HistoryError>> + Send;
}
impl ApplyHistory for HistoryBuffer {
    fn next_index(&self) -> HistoryIndex {
        self.next_index()
    }
    fn reset_with_index(&mut self, index: HistoryIndex) {
        self.reset_with_index(index);
    }
    async fn record_sync(
        &mut self,
        region: std::sync::Arc<astersql_pd_core::RegionInfo>,
    ) -> Result<bool, HistoryError> {
        self.record(region).map(|_| true)
    }
}
impl ApplyHistory for crate::PersistentHistory<'_> {
    fn next_index(&self) -> HistoryIndex {
        self.history().next_index()
    }
    fn reset_with_index(&mut self, index: HistoryIndex) {
        self.reset_with_index(index);
    }
    async fn record_sync(
        &mut self,
        region: std::sync::Arc<astersql_pd_core::RegionInfo>,
    ) -> Result<bool, HistoryError> {
        match self.record(region).await {
            Ok(_) => Ok(true),
            Err(crate::HistoryPersistError::History(error)) => Err(error),
            Err(crate::HistoryPersistError::Save { .. }) => Ok(false),
            Err(error) => unreachable!("record cannot load or decode a checkpoint: {error}"),
        }
    }
}

#[derive(Default)]
/// Follower streaming state. The host serializes responses and owns all resources.
pub struct ClientApply {
    running: bool,
    full_syncing: bool,
}
impl ClientApply {
    /// Whether the initial synchronization has completed.
    pub fn is_running(&self) -> bool {
        self.running
    }
    /// Whether nonempty full-sync frames still carry positional offsets.
    pub fn full_syncing(&self) -> bool {
        self.full_syncing
    }
    /// Start a new stream without discarding the caller-owned history or cache.
    pub fn reset_stream(&mut self) {
        self.running = false;
        self.full_syncing = false;
    }
    /// Apply a frame sequentially, skipping stale regions and error responses.
    /// Cache mutation precedes persistence. Save failures suppress history only;
    /// overlap deletion remains best effort. No background work is spawned.
    pub async fn apply(
        &mut self,
        response: SyncApplyResponse,
        cluster: &mut BasicCluster,
        storage: &Storage,
        paths: &ClusterPaths,
        history: &mut impl ApplyHistory,
    ) -> Result<ApplyOutcome, HistoryError> {
        if response.error.is_some() {
            self.running = false;
            return Ok(ApplyOutcome::default());
        }
        let start_full = !self.full_syncing
            && !self.running
            && response.start_index == HistoryIndex(0)
            && !response.regions.is_empty();
        let in_full = self.full_syncing || start_full;
        let positional = in_full && !start_full && !response.regions.is_empty();
        if !positional && history.next_index() != response.start_index {
            history.reset_with_index(response.start_index);
        }
        let has_regions = !response.regions.is_empty();
        let has_stats = response.stats.len() == response.regions.len();
        let has_buckets = response.buckets.len() == response.regions.len();
        let mut outcome = ApplyOutcome {
            handled: true,
            ..Default::default()
        };
        for (i, meta) in response.regions.into_iter().enumerate() {
            let leader = response
                .leaders
                .get(i)
                .cloned()
                .flatten()
                .filter(|p| p.id != 0);
            let mut region = astersql_pd_core::RegionInfo::new(meta, leader, vec![], vec![]);
            region.set_source(astersql_pd_core::RegionSource::Sync);
            if has_stats {
                let s = &response.stats[i];
                let target = region.stats_mut();
                target.written_bytes = s.written_bytes;
                target.written_keys = s.written_keys;
                target.read_bytes = s.read_bytes;
                target.read_keys = s.read_keys;
            }
            if has_buckets {
                region.set_bucket_meta(response.buckets[i].clone());
            }
            let (origin, _, precheck) = cluster.regions.pre_check_put_region(&region);
            if precheck.is_err() {
                outcome.stale += 1;
                continue;
            }
            let save = astersql_pd_core::region_guide(&region, origin.as_deref()).save_kv;
            let region = std::sync::Arc::new(region);
            let overlaps = cluster.regions.put_region(region.clone());
            outcome.applied += 1;
            let saved = !save || storage.save_region(paths, region.meta()).await.is_ok();
            if !saved {
                outcome.save_errors += 1;
            }
            // Go commits the cache before storage; failed saves suppress history,
            // but still delete overlaps. History exhaustion must not skip cleanup.
            let recorded = if saved && !in_full {
                history.record_sync(region).await.map(|checkpoint_ok| {
                    if !checkpoint_ok {
                        outcome.checkpoint_errors += 1;
                    }
                })
            } else {
                Ok(())
            };
            for old in overlaps {
                if storage.delete_region(paths, old.meta()).await.is_err() {
                    outcome.delete_errors += 1;
                }
            }
            recorded?;
        }
        self.full_syncing = in_full && has_regions;
        if !self.full_syncing {
            self.running = true;
        }
        Ok(outcome)
    }
}
