// Copyright 2026 AsterSQL.
//! Disk slow-store detection and recovery over canonical cluster and storage state.
use crate::{
    EvictLeaderConfig, EvictLeaderContext, EvictLeaderResult, EvictLeaderScheduler,
    InvalidLeaderBatch, SchedulerError, encode_config,
};
use astersql_pd_core::{KeyRange, StoreControlError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_storage_domain::StorageEndpoint;
const NAME: &str = "evict-slow-store-scheduler";

/// Only disk configuration is persisted here; recovery timestamps are process-local in Go.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "DiskJson", into = "DiskJson")]
pub struct EvictDiskConfig {
    recovery_sec: u64,
    batch: usize,
    evicted: Vec<u64>,
    last_capture: Option<i128>,
    is_recovered: bool,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct DiskJson {
    #[serde(rename = "recovery-duration", default)]
    recovery: u64,
    #[serde(rename = "evict-stores", default)]
    stores: Option<Vec<u64>>,
    #[serde(default)]
    batch: usize,
}
impl TryFrom<DiskJson> for EvictDiskConfig {
    type Error = InvalidLeaderBatch;
    fn try_from(v: DiskJson) -> Result<Self, Self::Error> {
        let mut conf = Self::new(v.recovery, if v.batch == 0 { 3 } else { v.batch })?;
        conf.evicted = v.stores.unwrap_or_default();
        Ok(conf)
    }
}
impl From<EvictDiskConfig> for DiskJson {
    fn from(v: EvictDiskConfig) -> Self {
        Self {
            recovery: v.recovery_sec,
            stores: Some(v.evicted),
            batch: v.batch,
        }
    }
}
impl Default for EvictDiskConfig {
    fn default() -> Self {
        Self::new(1800, 3).expect("valid default batch")
    }
}
impl EvictDiskConfig {
    pub fn new(recovery_sec: u64, batch: usize) -> Result<Self, InvalidLeaderBatch> {
        EvictLeaderConfig::new(Default::default(), batch)?;
        Ok(Self {
            recovery_sec,
            batch,
            evicted: vec![],
            last_capture: None,
            is_recovered: false,
        })
    }
    /// Change policy without resetting runtime recovery or persisted eviction state.
    pub(crate) fn update_policy(
        &mut self,
        recovery_sec: u64,
        batch: usize,
    ) -> Result<(), InvalidLeaderBatch> {
        EvictLeaderConfig::new(Default::default(), batch)?;
        self.recovery_sec = recovery_sec;
        self.batch = batch;
        Ok(())
    }
    pub fn evict_store(&self) -> u64 {
        self.evicted.first().copied().unwrap_or(0)
    }
    pub fn batch(&self) -> usize {
        self.batch
    }
    pub fn recovery_sec(&self) -> u64 {
        self.recovery_sec
    }
    pub fn last_capture(&self) -> Option<i128> {
        self.last_capture
    }
    pub fn is_recovered(&self) -> bool {
        self.is_recovered
    }
    /// The caller supplies a monotonic nondecreasing clock in Unix nanoseconds.
    pub fn ready_for_recovery(&self, now: i128) -> bool {
        let elapsed = self.last_capture.map_or(i128::from(i64::MAX), |capture| {
            now.saturating_sub(capture).min(i128::from(i64::MAX))
        });
        elapsed >= i128::from(self.recovery_sec) * 1_000_000_000
    }
}
#[derive(Debug, thiserror::Error)]
pub enum EvictDiskError {
    #[error(transparent)]
    Store(#[from] StoreControlError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
}
/// Keeps Go's disk state machine separate from its network-slow policy and HTTP adapter.
pub struct EvictDiskScheduler {
    config: EvictDiskConfig,
}
impl EvictDiskScheduler {
    pub fn new(config: EvictDiskConfig) -> Self {
        Self { config }
    }
    /// Apply an already persisted HTTP policy without resetting eviction or clocks.
    pub fn update_policy(
        &mut self,
        recovery_sec: u64,
        batch: usize,
    ) -> Result<(), InvalidLeaderBatch> {
        self.config.update_policy(recovery_sec, batch)
    }
    pub fn config(&self) -> &EvictDiskConfig {
        &self.config
    }
    pub fn is_schedule_allowed(&self, count: u64, limit: u64) -> bool {
        self.config.evict_store() == 0 || count < limit
    }
    pub fn prepare<C: BasicCluster>(&self, cluster: &C) -> Result<(), StoreControlError> {
        let id = self.config.evict_store();
        if id != 0 {
            cluster.slow_store_evicted(id)?;
        }
        Ok(())
    }
    async fn persist(
        &mut self,
        next: EvictDiskConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictDiskError> {
        let bytes = encode_config(&next)?;
        storage
            .save_scheduler_config(paths, NAME, &bytes)
            .await
            .map_err(SchedulerError::from)?;
        self.config = next;
        Ok(())
    }
    /// Go clears the cluster eviction reference even when clearing storage fails.
    /// Return that error to the owner while retaining the old configuration snapshot.
    pub async fn clean<C: BasicCluster>(
        &mut self,
        cluster: &C,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictDiskError> {
        let id = self.config.evict_store();
        if id == 0 {
            return Ok(());
        }
        let mut next = self.config.clone();
        next.evicted.clear();
        next.last_capture = None;
        let result = self.persist(next, storage, paths).await;
        cluster.slow_store_recovered(id);
        result
    }
    /// Detect a unique eligible slow store, or advance the existing recovery window.
    pub async fn observe<C: BasicCluster>(
        &mut self,
        cluster: &C,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        now: i128,
    ) -> Result<(), EvictDiskError> {
        let id = self.config.evict_store();
        if id != 0 {
            let store = cluster.get_store(id);
            let Some(store) = store.filter(|s| !s.is_removed()) else {
                return self.clean(cluster, storage, paths).await;
            };
            let recovered = store.slow_score() <= 1;
            if self.config.is_recovered != recovered {
                let mut next = self.config.clone();
                next.is_recovered = recovered;
                next.last_capture = Some(now);
                self.persist(next, storage, paths).await?;
            }
            if recovered && self.config.ready_for_recovery(now) {
                self.clean(cluster, storage, paths).await?;
            }
            return Ok(());
        }
        let mut candidate = None;
        for store in cluster.get_stores() {
            if !store.is_removed()
                && (store.is_preparing() || store.is_serving())
                && store.is_slow()
            {
                if candidate.is_some() {
                    return Ok(());
                }
                candidate = Some(store);
            }
        }
        let Some(store) = candidate.filter(|s| s.slow_score() >= 100) else {
            return Ok(());
        };
        let id = store.id();
        cluster.slow_store_evicted(id)?;
        // Publishing is deferred until the I/O succeeds. Drop rolls back acquisition on cancellation.
        let mut guard = EvictionGuard {
            cluster,
            id,
            committed: false,
        };
        let mut next = self.config.clone();
        next.evicted = vec![id];
        next.last_capture = Some(now);
        self.persist(next, storage, paths).await?;
        guard.committed = true;
        Ok(())
    }
    /// State errors do not suppress leader eviction from the retained snapshot, matching Go.
    pub async fn schedule<C: BasicCluster, P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut EvictLeaderContext<'_, C, P>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> (EvictLeaderResult, Option<EvictDiskError>) {
        let error = self
            .observe(ctx.cluster, storage, paths, ctx.now_unix_nanos)
            .await
            .err();
        let stores = self
            .config
            .evicted
            .iter()
            .map(|id| (*id, vec![KeyRange::default()]))
            .collect();
        let scheduler = EvictLeaderScheduler::new(
            EvictLeaderConfig::new(stores, self.config.batch).expect("validated disk batch"),
        );
        (scheduler.schedule_with_description(ctx, NAME).await, error)
    }
}
struct EvictionGuard<'a, C: BasicCluster> {
    cluster: &'a C,
    id: u64,
    committed: bool,
}
impl<C: BasicCluster> Drop for EvictionGuard<'_, C> {
    fn drop(&mut self) {
        if !self.committed {
            self.cluster.slow_store_recovered(self.id);
        }
    }
}
