// Copyright 2026 AsterSQL.
//! Slow-trend observation, durable eviction configuration and recovery.
use crate::{
    EvictLeaderConfig, EvictLeaderContext, EvictLeaderResult, EvictLeaderScheduler, IntervalGrowth,
    MAX_SCHEDULE_INTERVAL, MIN_SCHEDULE_INTERVAL, SchedulerError, SlowTrendCandidates,
    choose_slow_trend_candidate, encode_config, interval_grow, slow_trend_faster_than_others,
    slow_trend_stores_updated,
};
use astersql_pd_core::{BasicCluster, KeyRange, StoreControlError};
use astersql_pd_core_constant::Direction;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::SchedulerConfigProvider;
use astersql_pd_storage_endpoint::StorageEndpoint;
use std::collections::HashSet;
const NAME: &str = "evict-slow-trend-scheduler";

/// Only policy and evicted stores survive restart; candidate clocks are local.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SlowTrendConfig {
    #[serde(rename = "recovery-duration", default)]
    pub recovery_sec: u64,
    #[serde(rename = "evict-by-trend-stores", default)]
    pub evicted_stores: Vec<u64>,
}
impl Default for SlowTrendConfig {
    fn default() -> Self {
        Self {
            recovery_sec: 1800,
            evicted_stores: vec![],
        }
    }
}
impl SlowTrendConfig {
    pub fn evicted_store(&self) -> u64 {
        self.evicted_stores.first().copied().unwrap_or(0)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum SlowTrendError {
    #[error(transparent)]
    Store(#[from] StoreControlError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("slow trend candidate store {0} is missing")]
    MissingCandidate(u64),
}
/// Serialized by the host; uses canonical store controls and the shared leader builder.
pub struct SlowTrendScheduler {
    config: SlowTrendConfig,
    candidates: SlowTrendCandidates,
}
impl SlowTrendScheduler {
    pub fn new(config: SlowTrendConfig) -> Self {
        Self {
            config,
            candidates: SlowTrendCandidates::default(),
        }
    }
    pub fn config(&self) -> &SlowTrendConfig {
        &self.config
    }
    pub fn candidates(&self) -> &SlowTrendCandidates {
        &self.candidates
    }
    pub fn ready_for_recovery(&self, now: i64) -> bool {
        // Go time.Since(zero time) saturates at time.Duration::MAX after restart.
        self.candidates
            .last
            .map_or(i64::MAX as u64 / 1_000_000_000, |c| c.captured_secs(now))
            >= self.config.recovery_sec
    }
    /// Go ignores the supplied previous interval, growing from the minimum every time.
    pub fn next_interval(&self, _previous: i64) -> i64 {
        interval_grow(
            MIN_SCHEDULE_INTERVAL,
            MAX_SCHEDULE_INTERVAL,
            if self.config.evicted_stores.is_empty() {
                IntervalGrowth::Exponential
            } else {
                IntervalGrowth::Zero
            },
        )
    }
    pub fn is_schedule_allowed(&self, count: u64, limit: u64) -> bool {
        self.config.evicted_store() == 0 || count < limit
    }
    async fn save(
        &self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SlowTrendError> {
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&self.config)?)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
    /// Numeric policy updates roll back on storage failure, unlike eviction-set writes.
    pub async fn update_recovery(
        &mut self,
        seconds: u64,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), SlowTrendError> {
        let mut next = self.config.clone();
        next.recovery_sec = seconds;
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&next)?)
            .await
            .map_err(SchedulerError::from)?;
        self.config = next;
        Ok(())
    }
    pub fn prepare(&self, cluster: &BasicCluster) -> Result<(), StoreControlError> {
        let id = self.config.evicted_store();
        if id != 0 {
            cluster.stores.slow_trend_evicted(id)?;
        }
        Ok(())
    }
    /// Reload changes incoming leader pause references, as the Go reload path does.
    pub fn reload(
        &mut self,
        next: SlowTrendConfig,
        cluster: &BasicCluster,
    ) -> Vec<StoreControlError> {
        let old: HashSet<_> = self.config.evicted_stores.iter().copied().collect();
        let new: HashSet<_> = next.evicted_stores.iter().copied().collect();
        for id in old.difference(&new) {
            cluster.stores.resume_leader_transfer(*id, Direction::IN);
        }
        let mut errors = vec![];
        for id in new.difference(&old) {
            if let Err(e) = cluster.stores.pause_leader_transfer(*id, Direction::IN) {
                errors.push(e);
            }
        }
        self.config = next;
        errors
    }
    /// Clear memory before saving; always release core eviction even if saving fails.
    pub async fn clean(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        now: i64,
    ) -> Result<(), SlowTrendError> {
        let id = self.config.evicted_store();
        if id == 0 {
            return Ok(());
        }
        self.config.evicted_stores.clear();
        let _recovery = RecoveryGuard {
            cluster,
            candidates: &mut self.candidates,
            id,
            now,
        };
        storage
            .save_scheduler_config(paths, NAME, &encode_config(&self.config)?)
            .await
            .map_err(SchedulerError::from)?;
        Ok(())
    }
    /// Returns whether this round should build eviction operators. Clock is Unix nanoseconds.
    pub async fn observe(
        &mut self,
        cluster: &BasicCluster,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        now: i64,
        raft_v2: bool,
        affected_ratio: f64,
    ) -> Result<bool, SlowTrendError> {
        let id = self.config.evicted_store();
        if id != 0 {
            let store = cluster.stores.get_store(id);
            if store.as_ref().is_none_or(|s| {
                s.is_removed()
                    || (slow_trend_faster_than_others(cluster, s) && self.ready_for_recovery(now))
            }) {
                self.clean(cluster, storage, paths, now).await?;
                return Ok(false);
            }
            return Ok(true);
        }
        let fresh = self.candidates.current.is_none();
        if fresh {
            let Some(store) = choose_slow_trend_candidate(
                cluster,
                raft_v2,
                affected_ratio,
                self.candidates.last.as_ref(),
                now,
            ) else {
                return Ok(false);
            };
            self.candidates.capture(store.id(), now, now);
        }
        let candidate = self.candidates.current.expect("captured candidate");
        let store = cluster
            .stores
            .get_store(candidate.store_id)
            .ok_or(SlowTrendError::MissingCandidate(candidate.store_id))?;
        if !fresh && slow_trend_faster_than_others(cluster, &store) {
            self.candidates.pop(false);
            return Ok(false);
        }
        if !slow_trend_stores_updated(cluster, candidate.store_id, candidate.capture_ts) {
            return Ok(false);
        }
        let id = self.candidates.pop(true);
        self.config.evicted_stores = vec![id];
        self.save(storage, paths).await?;
        cluster.stores.slow_trend_evicted(id)?;
        Ok(true)
    }
    pub async fn schedule<P: SchedulerConfigProvider>(
        &mut self,
        ctx: &mut EvictLeaderContext<'_, BasicCluster, P>,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        now: i64,
        raft_v2: bool,
        affected_ratio: f64,
    ) -> (EvictLeaderResult, Option<SlowTrendError>) {
        match self
            .observe(ctx.cluster, storage, paths, now, raft_v2, affected_ratio)
            .await
        {
            Err(e) => (EvictLeaderResult::default(), Some(e)),
            Ok(false) => (EvictLeaderResult::default(), None),
            Ok(true) => {
                let stores = [(self.config.evicted_store(), vec![KeyRange::default()])].into();
                let scheduler = EvictLeaderScheduler::new(
                    EvictLeaderConfig::new(stores, 3).expect("fixed batch"),
                );
                (scheduler.schedule_with_description(ctx, NAME).await, None)
            }
        }
    }
}

// Release core state even when the host cancels an in-flight cleanup write.
struct RecoveryGuard<'a> {
    cluster: &'a BasicCluster,
    candidates: &'a mut SlowTrendCandidates,
    id: u64,
    now: i64,
}
impl Drop for RecoveryGuard<'_> {
    fn drop(&mut self) {
        self.candidates.mark_recovered(self.now);
        self.cluster.stores.slow_trend_recovered(self.id);
    }
}
