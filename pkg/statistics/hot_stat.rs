// Copyright 2026 AsterSQL.

use crate::{
    HotCache, HotPeerSnapshot, HotPeerStat, HotTask, HotTaskError, HotTaskOutput,
    RegionStatInformer, SlowStat, StoreKindLoads, StoreStatInformer, StoresStats,
};
use astersql_pd_core::RegionInfo;
use astersql_pd_statistics_buckets::{
    BucketRangeIndex, BucketStats, BucketSubmitError, BucketWaitError, BucketWorker,
    BucketWorkerClient,
};
use astersql_pd_statistics_utils::RWType;
use prometheus::Registry;
use std::collections::{BTreeMap, HashMap};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

/// Failure to obtain a complete snapshot; partial results are never published.
#[derive(Debug)]
pub enum StatisticsSnapshotError {
    Hot(HotTaskError),
    BucketSubmit(BucketSubmitError),
    BucketWait(BucketWaitError),
}
impl std::fmt::Display for StatisticsSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics snapshot {self:?}")
    }
}
impl std::error::Error for StatisticsSnapshotError {}
/// Detached scheduler input. Mutating a returned value cannot change a live cache.
#[derive(Default, Debug)]
pub struct StatisticsSnapshot {
    pub stores: HashMap<u64, StoreKindLoads>,
    pub read: BTreeMap<u64, Vec<HotPeerStat>>,
    pub write: BTreeMap<u64, Vec<HotPeerStat>>,
    pub buckets: BucketStats,
    pub exists_slow_stores: bool,
}
impl StoreStatInformer for StatisticsSnapshot {
    fn get_stores_loads(&self) -> HashMap<u64, StoreKindLoads> {
        self.stores.clone()
    }
}
impl RegionStatInformer for StatisticsSnapshot {
    fn get_hot_peer_stat(&self, rw: RWType, region: u64, store: u64) -> Option<HotPeerSnapshot> {
        let peers = match rw {
            RWType::READ => &self.read,
            RWType::WRITE => &self.write,
            _ => return None,
        };
        peers
            .get(&store)?
            .iter()
            .find(|p| p.region_id == region)
            .map(|p| HotPeerSnapshot {
                loads: std::array::from_fn(|i| p.get_load(i as isize)),
            })
    }
}
impl StatisticsSnapshot {
    /// Go hot stats filtering, retaining empty entries for stores with cold peers.
    pub fn get_hot_peer_stats(
        &self,
        rw: RWType,
        min_degree: i64,
    ) -> BTreeMap<u64, Vec<HotPeerStat>> {
        let peers = match rw {
            RWType::READ => &self.read,
            RWType::WRITE => &self.write,
            _ => return BTreeMap::new(),
        };
        peers
            .iter()
            .map(|(&id, peers)| {
                (
                    id,
                    peers
                        .iter()
                        .filter(|p| {
                            p.hot_degree >= min_degree
                                && !p.in_cold
                                && p.anti_count == rw.default_anti_count() as i64
                        })
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }
    /// Checks current membership and degree in either direction, including cold peers.
    pub fn is_region_hot(&self, region: &RegionInfo, min_degree: i64) -> bool {
        [&self.read, &self.write].into_iter().any(|stats| {
            region.meta().peers.iter().any(|member| {
                stats.get(&member.store_id).is_some_and(|peers| {
                    peers
                        .iter()
                        .any(|p| p.region_id == region.meta().id && p.hot_degree >= min_degree)
                })
            })
        })
    }
}
/// Composes the canonical caches on a host runtime, including independent slow state.
pub struct HotStat {
    pub hot_cache: HotCache,
    pub stores_stats: StoresStats,
    pub slow_stat: SlowStat,
    bucket_worker: BucketWorker,
    stopped: CancellationToken,
}
impl HotStat {
    /// Registers buckets before starting hot workers so registration failure leaks no tasks.
    pub fn new(
        handle: &Handle,
        registry: &Registry,
        capacity: usize,
    ) -> Result<Self, prometheus::Error> {
        assert!(capacity > 0, "hot queue capacity must be positive");
        let stopped = CancellationToken::new();
        let bucket_worker =
            BucketWorker::start(handle, &stopped, registry, BucketRangeIndex::new())?;
        Ok(Self {
            hot_cache: HotCache::new(handle, capacity),
            stores_stats: StoresStats::new(),
            slow_stat: SlowStat::new(),
            bucket_worker,
            stopped,
        })
    }
    pub fn bucket_client(&self) -> BucketWorkerClient {
        self.bucket_worker.client()
    }
    /// Queries follow previously admitted reports. Cross-cache concurrent writes are not atomic.
    pub async fn snapshot(
        &self,
        min_bucket_degree: i32,
        region_ids: Vec<u64>,
    ) -> Result<StatisticsSnapshot, StatisticsSnapshotError> {
        // Admit every FIFO barrier before waiting for results. Failed admission
        // may leave harmless read queries queued, but never returns a partial snapshot.
        let read = self
            .hot_cache
            .submit(RWType::READ, HotTask::Snapshot, 0)
            .map_err(StatisticsSnapshotError::Hot)?;
        let write = self
            .hot_cache
            .submit(RWType::WRITE, HotTask::Snapshot, 0)
            .map_err(StatisticsSnapshotError::Hot)?;
        let buckets = self
            .bucket_client()
            .collect(min_bucket_degree, region_ids)
            .map_err(StatisticsSnapshotError::BucketSubmit)?;
        let stores = self.stores_stats.get_stores_loads();
        let exists_slow_stores = self.slow_stat.exists_slow_stores();
        let (read, write, buckets) =
            tokio::join!(read.wait(), write.wait(), buckets.wait(&self.stopped));
        let HotTaskOutput::Stats(read) = read.map_err(StatisticsSnapshotError::Hot)? else {
            unreachable!("snapshot command")
        };
        let HotTaskOutput::Stats(write) = write.map_err(StatisticsSnapshotError::Hot)? else {
            unreachable!("snapshot command")
        };
        let buckets = buckets.map_err(StatisticsSnapshotError::BucketWait)?;
        Ok(StatisticsSnapshot {
            stores,
            read,
            write,
            buckets,
            exists_slow_stores,
        })
    }
    /// Stops admission to all workers without canceling a host-owned runtime.
    pub fn cancel(&self) {
        self.hot_cache.cancel();
        self.stopped.cancel();
    }
    /// Joins every child even if another child failed. Safe to repeat.
    pub async fn shutdown(&mut self) -> Result<(), StatisticsSnapshotError> {
        self.cancel();
        let (hot, bucket) = tokio::join!(self.hot_cache.shutdown(), self.bucket_worker.shutdown());
        hot.map_err(StatisticsSnapshotError::Hot)?;
        bucket.map_err(|_| StatisticsSnapshotError::BucketWait(BucketWaitError::WorkerStopped))?;
        Ok(())
    }
}
impl StoreStatInformer for HotStat {
    fn get_stores_loads(&self) -> HashMap<u64, StoreKindLoads> {
        self.stores_stats.get_stores_loads()
    }
}
impl Drop for HotStat {
    fn drop(&mut self) {
        self.cancel();
    }
}
