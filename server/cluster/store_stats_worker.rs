// Copyright 2026 AsterSQL.
//! Host-owned periodic refresh of canonical store counters.
use crate::StoreStateWorker;
use std::{future::Future, time::Duration};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

/// Statistics job with the same cancellation/join discipline as lifecycle checks.
pub struct StoreStatsWorker(StoreStateWorker);
impl StoreStatsWorker {
    /// Wait one Go-compatible 9ms period before refreshing the shared indexes.
    pub fn start<F, Fut>(executor: &Handle, parent: &CancellationToken, refresh: F) -> Self
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Self(StoreStateWorker::start(
            executor,
            parent,
            Duration::from_millis(9),
            refresh,
        ))
    }
    /// Cancel and await any admitted refresh before releasing host resources.
    pub async fn stop(self) -> Result<(), tokio::task::JoinError> {
        self.0.stop().await
    }
}

/// Refresh counters from the canonical region indexes without replaying samples.
pub fn refresh_store_stats(cluster: &astersql_pd_core::BasicCluster) {
    cluster.update_all_store_status();
}
