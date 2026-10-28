// Copyright 2026 AsterSQL.
//! Minimum resolved timestamp aggregation over the canonical store collection.
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{StorageEndpoint, TimestampValueError};
use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Advance memory before persistence, including when persistence fails (Go contract).
/// The host must serialize calls with its single aggregation worker.
pub async fn refresh_min_resolved_ts(
    candidate: Option<u64>,
    current: &AtomicU64,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
) -> Result<(), TimestampValueError> {
    if let Some(value) =
        candidate.filter(|value| *value != u64::MAX && *value > current.load(Ordering::Relaxed))
    {
        current.store(value, Ordering::Relaxed);
        storage.save_min_resolved_ts(paths, value).await?;
    }
    Ok(())
}

/// Snapshot eligible stores under the host's admission lock.
pub fn min_resolved_ts_candidate(cluster: &BasicCluster, max_replicas: usize) -> Option<u64> {
    if cluster.regions.total_region_count() <= 1
        && !cluster.regions.get_regions().iter().any(|r| {
            r.meta().key_range.start_key.is_empty()
                && r.voters().len() >= max_replicas
                && r.pending_peers().is_empty()
        })
    {
        return None;
    }
    cluster
        .stores
        .get_stores()
        .iter()
        .filter(|s| s.is_available_for_min_resolved_ts())
        .map(|s| s.min_resolved_ts())
        .min()
        .filter(|v| *v != u64::MAX)
}

/// Host-owned worker; cancellation joins an admitted storage transaction.
pub struct ResolvedTsWorker {
    cancel: CancellationToken,
    join: Option<JoinHandle<()>>,
}
impl Drop for ResolvedTsWorker {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl ResolvedTsWorker {
    pub fn start<I, F, Fut>(
        executor: &Handle,
        parent: &CancellationToken,
        interval: I,
        mut refresh: F,
    ) -> Self
    where
        I: Fn() -> Duration + Send + 'static,
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = parent.child_token();
        let token = cancel.clone();
        let join = executor.spawn(async move {
            let period = |d: Duration| if d.is_zero() { Duration::from_secs(1) } else { d };
            let mut delay = period(interval());
            loop {
                tokio::select! { biased; _ = token.cancelled() => break, _ = tokio::time::sleep(delay) => {} }
                let configured = interval();
                if !configured.is_zero() { refresh().await; }
                delay = period(configured);
            }
        });
        Self {
            cancel,
            join: Some(join),
        }
    }
    pub async fn stop(mut self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        self.join.take().expect("worker join handle").await
    }
}
