// Copyright 2026 AsterSQL.
//! Host-owned availability task and compatible, registry-scoped metrics.
use crate::manager::Manager;
use astersql_pd_core::StoreInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use prometheus::{IntGauge, IntGaugeVec, Opts, Registry};
use std::{sync::Arc, time::Duration};
use tokio::{runtime::Handle, sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_AVAILABILITY_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Cached fixed-label children; no process-global registration.
#[derive(Clone, Debug)]
pub struct AffinityMetrics {
    groups: IntGauge,
    regions: IntGauge,
    affinity_regions: IntGauge,
    split_denied: IntGauge,
}
impl AffinityMetrics {
    fn register(registry: &Registry) -> Result<(Self, IntGaugeVec), prometheus::Error> {
        let vector = IntGaugeVec::new(
            Opts::new("pd_affinity_status", "Status of the affinity manager."),
            &["type"],
        )?;
        let metrics = Self {
            groups: vector.with_label_values(&["group-count"]),
            regions: vector.with_label_values(&["region-count"]),
            affinity_regions: vector.with_label_values(&["affinity-region-count"]),
            split_denied: vector.with_label_values(&["split-deny-count"]),
        };
        registry.register(Box::new(vector.clone()))?;
        Ok((metrics, vector))
    }
    pub(crate) fn record_split_deny_metrics(&self) {
        self.split_denied.inc();
    }
    fn collect_metrics(&self, manager: &Manager) {
        self.groups.set(manager.groups.len() as i64);
        self.regions.set(manager.regions.len() as i64);
        self.affinity_regions
            .set(manager.affinity_region_count() as i64);
    }
}
struct Registration {
    registry: Registry,
    vector: IntGaugeVec,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let _ = self.registry.unregister(Box::new(self.vector.clone()));
    }
}
/// Explicit shutdown cancels and joins; dropping also cancels the task.
/// Initialize Manager before calling start: failed restoration never spawns a task.
pub struct AvailabilityLoop {
    pub manager: Arc<Mutex<Manager>>,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Restore(#[from] crate::manager::RestoreError),
    #[error(transparent)]
    Metrics(#[from] prometheus::Error),
}
impl AvailabilityLoop {
    /// Await real restoration before acquiring metrics or spawning the loop.
    pub async fn initialize<C, F>(
        handle: &Handle,
        parent: &CancellationToken,
        registry: Registry,
        interval: Duration,
        restore: impl std::future::Future<Output = Result<Manager, crate::manager::RestoreError>>,
        snapshot: F,
    ) -> Result<Self, StartError>
    where
        C: SharedConfigProvider + Send + 'static,
        F: Fn() -> (Vec<StoreInfo>, C, i128, u64) + Send + 'static,
    {
        Ok(Self::start(
            handle,
            parent,
            registry,
            interval,
            restore.await?,
            snapshot,
        )?)
    }

    /// Snapshot supplies live stores, configuration and Unix nanoseconds/seconds.
    /// The interval uses the host Tokio clock, so deterministic clocks need no failpoint.
    pub fn start<C, F>(
        handle: &Handle,
        parent: &CancellationToken,
        registry: Registry,
        interval: Duration,
        mut manager: Manager,
        snapshot: F,
    ) -> Result<Self, prometheus::Error>
    where
        C: SharedConfigProvider + Send + 'static,
        F: Fn() -> (Vec<StoreInfo>, C, i128, u64) + Send + 'static,
    {
        if interval.is_zero() {
            return Err(prometheus::Error::Msg(
                "availability interval must be positive".into(),
            ));
        }
        let (metrics, vector) = AffinityMetrics::register(&registry)?;
        manager.metrics = Some(metrics.clone());
        let manager = Arc::new(Mutex::new(manager));
        let shared = manager.clone();
        let cancel = parent.child_token();
        let stop = cancel.clone();
        let registration = Registration { registry, vector };
        let task = handle.spawn(async move {
            let _registration = registration;
            let mut ticker =
                tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased;
                    _ = stop.cancelled() => break,
                    _ = ticker.tick() => {
                        let mut manager = tokio::select! { biased;
                            _ = stop.cancelled() => break,
                            manager = shared.lock() => manager,
                        };
                        if !manager.groups.is_empty() {
                            let (stores, conf, nanos, seconds) = snapshot();
                            manager.check_groups_availability(&stores, &conf, nanos, seconds);
                            metrics.collect_metrics(&manager);
                        }
                    }
                }
            }
        });
        Ok(Self {
            manager,
            cancel,
            task: Some(task),
        })
    }
    pub async fn shutdown(&mut self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.await?;
        }
        Ok(())
    }
}
impl Drop for AvailabilityLoop {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
