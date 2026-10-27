// Copyright 2026 AsterSQL.
//! Network pause, eviction and continuous recovery using canonical store controls.
use crate::{
    EvictDiskConfig, EvictLeaderConfig, EvictLeaderContext, EvictLeaderResult,
    EvictLeaderScheduler, InvalidLeaderBatch, SchedulerError, encode_config,
    filter_network_slow_scores,
};
use astersql_pd_core::{BasicCluster, KeyRange, StoreControlError};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::collections::{BTreeMap, HashSet};
const NAME: &str = "evict-slow-store-scheduler";

/// Persisted network fields share Go's JSON envelope with disk fields.
/// Recovery clocks and waiting candidates deliberately remain process-local.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EvictNetworkConfig {
    #[serde(flatten)]
    pub disk: EvictDiskConfig,
    #[serde(rename = "enable-network-slow-store", default)]
    pub enabled: bool,
    #[serde(rename = "paused-network-slow-stores", default)]
    pub paused: Vec<u64>,
    #[serde(rename = "evicted-network-slow-stores", default)]
    pub evicted: Vec<u64>,
}
impl EvictNetworkConfig {
    pub fn new(enabled: bool, recovery_sec: u64, batch: usize) -> Result<Self, InvalidLeaderBatch> {
        Ok(Self {
            disk: EvictDiskConfig::new(recovery_sec, batch)?,
            enabled,
            paused: vec![],
            evicted: vec![],
        })
    }
}
/// Typed failures at store controls, storage/JSON, or configuration validation.
#[derive(Debug, thiserror::Error)]
pub enum EvictNetworkError {
    #[error(transparent)]
    Store(#[from] StoreControlError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error(transparent)]
    Batch(#[from] InvalidLeaderBatch),
}
/// This owner must be serialized with disk config updates by the host, as in Go's config lock.
pub struct EvictNetworkScheduler {
    config: EvictNetworkConfig,
    recovering: BTreeMap<u64, Option<i128>>,
}
impl EvictNetworkScheduler {
    pub fn new(config: EvictNetworkConfig) -> Self {
        Self {
            config,
            recovering: BTreeMap::new(),
        }
    }
    pub fn config(&self) -> &EvictNetworkConfig {
        &self.config
    }
    pub fn recovering(&self) -> &BTreeMap<u64, Option<i128>> {
        &self.recovering
    }
    async fn persist(
        &mut self,
        next: EvictNetworkConfig,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictNetworkError> {
        let bytes = encode_config(&next)?;
        storage
            .save_scheduler_config(paths, NAME, &bytes)
            .await
            .map_err(SchedulerError::from)?;
        self.config = next;
        Ok(())
    }
    /// Recovery clocks survive updates/reload.
    /// Reload adjusts pause references but intentionally does not rebuild eviction references.
    pub fn reload(
        &mut self,
        next: EvictNetworkConfig,
        cluster: &BasicCluster,
    ) -> Vec<StoreControlError> {
        let mut errors = vec![];
        let old: HashSet<_> = self
            .config
            .paused
            .iter()
            .copied()
            .chain((self.config.disk.evict_store() != 0).then_some(self.config.disk.evict_store()))
            .collect();
        let new: HashSet<_> = next
            .paused
            .iter()
            .copied()
            .chain((next.disk.evict_store() != 0).then_some(next.disk.evict_store()))
            .collect();
        for id in old.difference(&new) {
            cluster.stores.resume_leader_transfer(*id, Direction::IN);
        }
        for id in new.difference(&old) {
            if let Err(e) = cluster.stores.pause_leader_transfer(*id, Direction::IN) {
                errors.push(e);
            }
        }
        self.config = next;
        errors
    }
    /// Persist HTTP policy against the current disk snapshot; keep both owners aligned.
    /// Enablement deliberately survives I/O failure, matching Go persistLocked.
    pub async fn update_with_disk(
        &mut self,
        disk: &mut crate::EvictDiskScheduler,
        enabled: bool,
        recovery_sec: u64,
        batch: usize,
        disabled: bool,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictNetworkError> {
        let mut next = self.config.clone();
        next.disk = disk.config().clone();
        next.disk.update_policy(recovery_sec, batch)?;
        next.enabled = enabled;
        let mut envelope = serde_json::to_value(&next).map_err(SchedulerError::Encode)?;
        envelope["disabled"] = serde_json::Value::Bool(disabled);
        let bytes = encode_config(&envelope)?;
        // Go restores numeric policy on save failure but does not restore this flag.
        self.config.enabled = enabled;
        storage
            .save_scheduler_config(paths, NAME, &bytes)
            .await
            .map_err(SchedulerError::from)?;
        self.config = next;
        disk.update_policy(recovery_sec, batch)?;
        Ok(())
    }
    pub async fn update(
        &mut self,
        enabled: bool,
        recovery_sec: u64,
        batch: usize,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictNetworkError> {
        let mut next = self.config.clone();
        next.disk.update_policy(recovery_sec, batch)?;
        next.enabled = enabled;
        // Go persistLocked restores numeric policy and sets, but not EnableNetworkSlowStore.
        self.config.enabled = enabled;
        self.persist(next, storage, paths).await
    }
    pub fn prepare(&mut self, cluster: &BasicCluster) -> Vec<StoreControlError> {
        let mut errors = vec![];
        for &id in &self.config.paused {
            self.recovering.insert(id, None);
            if let Err(e) = cluster.stores.pause_leader_transfer(id, Direction::IN) {
                errors.push(e);
            }
        }
        for &id in &self.config.evicted {
            if let Err(e) = cluster.stores.slow_store_evicted(id) {
                errors.push(e);
            }
        }
        errors
    }
    async fn add(
        &mut self,
        id: u64,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictNetworkError> {
        cluster.stores.pause_leader_transfer(id, Direction::IN)?;
        let mut guard = ControlGuard {
            cluster,
            id,
            eviction: false,
            committed: false,
        };
        let mut next = self.config.clone();
        next.paused.push(id);
        self.persist(next, storage, paths).await?;
        self.recovering.insert(id, None);
        guard.committed = true;
        Ok(())
    }
    async fn delete(
        &mut self,
        id: u64,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), EvictNetworkError> {
        let evicted = self.config.evicted.contains(&id);
        let mut next = self.config.clone();
        next.paused.retain(|v| *v != id);
        next.evicted.retain(|v| *v != id);
        self.persist(next, storage, paths).await?;
        cluster.stores.resume_leader_transfer(id, Direction::IN);
        if evicted {
            cluster.stores.slow_store_recovered(id);
        }
        self.recovering.remove(&id);
        Ok(())
    }
    pub async fn clean(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Vec<EvictNetworkError> {
        let mut errors = vec![];
        for id in self.config.paused.clone() {
            if let Err(e) = self.delete(id, cluster, storage, paths).await {
                errors.push(e);
            }
        }
        errors
    }
    /// Explicit monotonic Unix-nanosecond clock; errors are returned while other stores advance.
    pub async fn observe(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        now: i128,
    ) -> Vec<EvictNetworkError> {
        if !self.config.enabled {
            let errors = self.clean(cluster, storage, paths).await;
            self.recovering.clear(); // Go clears waiting clocks even if a delete failed.
            return errors;
        }
        let mut errors = vec![];
        let mut reset = vec![];
        let mut start = vec![];
        for (id, since) in self.recovering.clone() {
            let store = cluster.stores.get_store(id);
            if store
                .as_ref()
                .is_none_or(|s| s.is_removed() || s.is_removing())
            {
                if let Err(e) = self.delete(id, cluster, storage, paths).await {
                    errors.push(e);
                }
                continue;
            }
            // Go mutates deletes during iteration; rebuild keys after successful removals.
            let current_keys = self.recovering.keys().copied().collect();
            let scores = filter_network_slow_scores(
                cluster,
                &store.unwrap().network_slow_scores(),
                &current_keys,
            );
            let average = if scores.is_empty() {
                0
            } else {
                scores.values().fold(0u64, |s, v| s.wrapping_add(*v)) / scores.len() as u64
            };
            if average > 1 {
                reset.push(id);
            } else if let Some(since) = since {
                if now.saturating_sub(since) / 1_000_000_000
                    >= i128::from(self.config.disk.recovery_sec())
                {
                    if let Err(e) = self.delete(id, cluster, storage, paths).await {
                        errors.push(e);
                    }
                }
            } else {
                start.push(id);
            }
        }
        for id in reset {
            self.recovering.insert(id, None);
        }
        for id in start {
            self.recovering.insert(id, Some(now));
        }
        if self.config.paused.is_empty() {
            if let Some(&id) = self.recovering.keys().next() {
                if let Err(e) = self.add(id, cluster, storage, paths).await {
                    errors.push(e);
                }
            }
        }
        let keys = self.recovering.keys().copied().collect();
        let at_capacity = !self.config.paused.is_empty();
        // Candidate snapshot mirrors Go's captured paused slice during this phase.
        let mut live: Vec<_> = cluster
            .stores
            .get_stores()
            .into_iter()
            .filter(|s| !s.is_removed())
            .collect();
        live.sort_by_key(|s| s.id());
        let graph = crate::build_problematic_network_map(cluster, &live, &keys);
        for store in &live {
            let keys = self.recovering.keys().copied().collect();
            if crate::should_skip_store_evaluation(store, &graph, &keys)
                || !crate::is_network_slow_store(cluster, store, &live, &graph, &keys)
            {
                continue;
            }
            let id = store.id();
            if cluster.stores.get_store(id).is_none_or(|s| s.is_removed()) {
                continue;
            }
            if at_capacity {
                self.recovering.insert(id, None);
            } else if let Err(e) = self.add(id, cluster, storage, paths).await {
                errors.push(e);
            }
        }
        for (&id, since) in &self.recovering.clone() {
            if since.is_some()
                || !self.config.paused.contains(&id)
                || self.config.evicted.contains(&id)
            {
                continue;
            }
            let Some(store) = cluster.stores.get_store(id) else {
                continue;
            };
            if store.network_slow_triggers() < 6 {
                continue;
            }
            if let Err(e) = cluster.stores.slow_store_evicted(id) {
                errors.push(e.into());
                continue;
            }
            let mut guard = ControlGuard {
                cluster,
                id,
                eviction: true,
                committed: false,
            };
            let mut next = self.config.clone();
            next.evicted.push(id);
            match self.persist(next, storage, paths).await {
                Ok(()) => guard.committed = true,
                Err(e) => errors.push(e),
            }
        }
        errors
    }
    pub async fn schedule<P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut EvictLeaderContext<'_, BasicCluster, P>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> (EvictLeaderResult, Vec<EvictNetworkError>) {
        let errors = self
            .observe(ctx.cluster, storage, paths, ctx.now_unix_nanos)
            .await;
        let stores = self
            .config
            .evicted
            .iter()
            .map(|id| (*id, vec![KeyRange::default()]))
            .collect();
        let scheduler = EvictLeaderScheduler::new(
            EvictLeaderConfig::new(stores, self.config.disk.batch())
                .expect("validated network batch"),
        );
        (scheduler.schedule_with_description(ctx, NAME).await, errors)
    }
}
struct ControlGuard<'a> {
    cluster: &'a BasicCluster,
    id: u64,
    eviction: bool,
    committed: bool,
}
impl Drop for ControlGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            if self.eviction {
                self.cluster.stores.slow_store_recovered(self.id);
            } else {
                self.cluster
                    .stores
                    .resume_leader_transfer(self.id, Direction::IN);
            }
        }
    }
}
