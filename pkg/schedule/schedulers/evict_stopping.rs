// Copyright 2026 AsterSQL.
//! Stopping-store detection and recovery over canonical cluster and storage state.
use crate::{
    EvictLeaderConfig, EvictLeaderContext, EvictLeaderResult, EvictLeaderScheduler,
    InvalidLeaderBatch, SchedulerError, encode_config,
};
use astersql_pd_core::{KeyRange, StoreControlError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_storage_endpoint::StorageEndpoint;
const NAME: &str = "evict-stopping-store-scheduler";

/// Persisted stopping-store snapshot; reload defaults a missing/zero batch to three.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "StoppingJson", into = "StoppingJson")]
pub struct EvictStoppingConfig {
    batch: usize,
    evicted: Vec<u64>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct StoppingJson {
    #[serde(rename = "evicted-stores", default)]
    stores: Option<Vec<u64>>,
    #[serde(default)]
    batch: usize,
}
impl TryFrom<StoppingJson> for EvictStoppingConfig {
    type Error = InvalidLeaderBatch;
    fn try_from(v: StoppingJson) -> Result<Self, Self::Error> {
        let mut conf = Self::new(if v.batch == 0 { 3 } else { v.batch })?;
        conf.evicted = v.stores.unwrap_or_default();
        Ok(conf)
    }
}
impl From<EvictStoppingConfig> for StoppingJson {
    fn from(v: EvictStoppingConfig) -> Self {
        Self {
            stores: Some(v.evicted),
            batch: v.batch,
        }
    }
}
impl Default for EvictStoppingConfig {
    fn default() -> Self {
        Self::new(3).expect("valid default batch")
    }
}
impl EvictStoppingConfig {
    pub fn new(batch: usize) -> Result<Self, InvalidLeaderBatch> {
        EvictLeaderConfig::new(Default::default(), batch)?;
        Ok(Self {
            batch,
            evicted: vec![],
        })
    }
    pub fn batch(&self) -> usize {
        self.batch
    }
    pub fn evict_store(&self) -> u64 {
        self.evicted.first().copied().unwrap_or(0)
    }
    pub fn evicted_stores(&self) -> &[u64] {
        &self.evicted
    }
}
#[derive(Debug, thiserror::Error)]
pub enum EvictStoppingError {
    #[error("batch is invalid, it should be in [1, 10]")]
    InvalidUpdate,
    #[error(transparent)]
    Store(#[from] StoreControlError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
}
/// Owns the stopping-store state machine; transport adapters remain outside this module.
pub struct EvictStoppingScheduler {
    config: EvictStoppingConfig,
}
impl EvictStoppingScheduler {
    pub fn new(config: EvictStoppingConfig) -> Self {
        Self { config }
    }
    pub fn config(&self) -> &EvictStoppingConfig {
        &self.config
    }
    pub fn is_schedule_allowed(&self, count: u64, limit: u64) -> bool {
        self.config.evict_store() == 0 || count < limit
    }
    pub fn prepare<C: BasicCluster>(&self, cluster: &C) -> Result<(), StoreControlError> {
        let id = self.config.evict_store();
        if id != 0 {
            cluster.stopping_store_evicted(id)?;
        }
        Ok(())
    }
    async fn persist(
        &mut self,
        next: EvictStoppingConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictStoppingError> {
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
    ) -> Result<(), EvictStoppingError> {
        let id = self.config.evict_store();
        if id == 0 {
            return Ok(());
        }
        let mut next = self.config.clone();
        next.evicted.clear();
        let result = self.persist(next, storage, paths).await;
        cluster.stopping_store_recovered(id);
        result
    }
    /// Reload only publishes configuration; Prepare owns cluster reference acquisition.
    pub fn reload(&mut self, next: EvictStoppingConfig) {
        self.config = next;
    }
    /// HTTP batch validation is 1..=10 (fractional JSON values truncate in Go).
    pub async fn update(
        &mut self,
        batch: f64,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictStoppingError> {
        if !batch.is_finite() || !(1.0..=10.0).contains(&batch) {
            return Err(EvictStoppingError::InvalidUpdate);
        }
        let mut next = self.config.clone();
        next.batch = batch as usize;
        self.persist(next, storage, paths).await
    }
    /// A single serving/preparing stopping node is selected; recovery never recaptures this round.
    pub async fn observe<C: BasicCluster>(
        &mut self,
        cluster: &C,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictStoppingError> {
        let id = self.config.evict_store();
        if id != 0 {
            if cluster
                .get_store(id)
                .is_none_or(|s| s.is_removed() || !s.store_stats().is_stopping)
            {
                return self.clean(cluster, storage, paths).await;
            }
            return Ok(());
        }
        let mut candidate = None;
        for store in cluster.get_stores() {
            if !store.is_removed()
                && (store.is_preparing() || store.is_serving())
                && store.store_stats().is_stopping
            {
                if candidate.is_some() {
                    return Ok(());
                }
                candidate = Some(store.id());
            }
        }
        let Some(id) = candidate else {
            return Ok(());
        };
        cluster.stopping_store_evicted(id)?;
        let mut guard = EvictionGuard {
            cluster,
            id,
            committed: false,
        };
        let mut next = self.config.clone();
        next.evicted = vec![id];
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
    ) -> (EvictLeaderResult, Option<EvictStoppingError>) {
        let error = self.observe(ctx.cluster, storage, paths).await.err();
        let stores = self
            .config
            .evicted
            .iter()
            .map(|id| (*id, vec![KeyRange::default()]))
            .collect();
        let scheduler = EvictLeaderScheduler::new(
            EvictLeaderConfig::new(stores, self.config.batch).expect("validated stopping batch"),
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
            self.cluster.stopping_store_recovered(self.id);
        }
    }
}
