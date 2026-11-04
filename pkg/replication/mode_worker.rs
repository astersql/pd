// Copyright 2026 AsterSQL.
//! Host-owned replication worker lifecycle and scalar metric handles.
use crate::{ModeError, ModeManager, ModeSnapshot};
use astersql_pd_core::{RegionsInfo, StoreInfo};
use astersql_pd_schedule_placement::Rule;
use prometheus::{Counter, Gauge, core::Collector};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Go-compatible scalar collectors, registered and retained only by the embedding host.
#[derive(Clone)]
pub struct ModeMetrics {
    pub ticks: Counter,
    pub state: Gauge,
    pub state_id: Gauge,
    pub recovered_regions: Gauge,
    pub recover_progress: Gauge,
}
impl ModeMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        Ok(Self {
            ticks: Counter::new(
                "pd_replication_dr_tick_total",
                "Counter of background state check count",
            )?,
            state: Gauge::new("pd_replication_dr_state", "State of DR")?,
            state_id: Gauge::new("pd_replication_dr_state_id", "State ID of DR")?,
            recovered_regions: Gauge::new(
                "pd_replication_dr_recovered_region",
                "Number of recovered regions",
            )?,
            recover_progress: Gauge::new(
                "pd_replication_dr_recover_progress",
                "Progress of sync_recover process",
            )?,
        })
    }
    pub fn collectors(&self) -> Vec<Box<dyn Collector>> {
        vec![
            Box::new(self.ticks.clone()),
            Box::new(self.state.clone()),
            Box::new(self.state_id.clone()),
            Box::new(self.recovered_regions.clone()),
            Box::new(self.recover_progress.clone()),
        ]
    }
    /// Observe the same publication used for member file replication.
    pub fn publish(&self, snapshot: &ModeSnapshot) {
        if snapshot.config.replication_mode != "dr-auto-sync" {
            return;
        }
        self.state.set(match snapshot.dr_auto_sync.state.as_str() {
            "sync" => 1.0,
            "async_wait" => 2.0,
            "async" => 3.0,
            "sync_recover" => 4.0,
            _ => 0.0,
        });
        self.state_id.set(snapshot.dr_auto_sync.state_id as f64);
    }
}
impl ModeManager {
    /// Bind host-owned collectors before starting the one worker for this manager.
    pub fn set_metrics(&self, metrics: ModeMetrics) {
        self.metrics.get_or_init(|| metrics);
    }
    /// Execute exactly one edge, like one Go state-check tick.
    pub async fn tick(
        &self,
        stores: &[Arc<StoreInfo>],
        rules: &[Rule],
        regions: &RegionsInfo,
        now: i128,
    ) -> Result<bool, ModeError> {
        let state = self.snapshot().await;
        if state.config.replication_mode != "dr-auto-sync" {
            return Ok(false);
        }
        if let Some(metrics) = self.metrics.get() {
            metrics.ticks.inc();
        }
        match state.dr_auto_sync.state.as_str() {
            "sync" => self.tick_sync(stores, rules, now).await,
            "async_wait" => self.tick_async_wait(stores, rules, now).await,
            "async" => self.tick_async(stores, rules, now).await,
            "sync_recover" => self.tick_recover(stores, rules, regions, now).await,
            _ => Ok(false),
        }
    }
}
/// Both periodic loops belong to a single joined host task.
#[derive(Clone, Copy)]
pub enum ModePhase {
    Update,
    Replicate,
}
struct ResetState(Option<ModeMetrics>);
impl Drop for ResetState {
    fn drop(&mut self) {
        if let Some(metrics) = &self.0 {
            metrics.state.set(0.0);
        }
    }
}
pub struct ModeWorker {
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl ModeWorker {
    /// Start on the supplied executor. Go waits one minute, then checks every 500ms
    /// and replicates files every five seconds; no private runtime or global state.
    pub fn start<F, Fut>(
        handle: &Handle,
        parent: &CancellationToken,
        metrics: Option<ModeMetrics>,
        callback: F,
    ) -> Self
    where
        F: Fn(ModePhase) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let cancel = parent.child_token();
        let token = cancel.clone();
        let task = handle.spawn(async move {
            tokio::select! {
                biased;
                _ = token.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            let _reset_state = ResetState(metrics);
            let update = async {
                let period = Duration::from_millis(500);
                let mut timer =
                    tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => break,
                        _ = timer.tick() => {
                            tokio::select! {
                                biased;
                                _ = token.cancelled() => break,
                                _ = callback(ModePhase::Update) => {}
                            }
                        }
                    }
                }
            };
            let replicate = async {
                let period = Duration::from_secs(5);
                let mut timer =
                    tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => break,
                        _ = timer.tick() => {
                            tokio::select! {
                                biased;
                                _ = token.cancelled() => break,
                                _ = callback(ModePhase::Replicate) => {}
                            }
                        }
                    }
                }
            };
            tokio::join!(update, replicate);
        });
        Self {
            cancel,
            task: Some(task),
        }
    }
    pub async fn stop(mut self) -> Result<(), tokio::task::JoinError> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.await?;
        }
        Ok(())
    }
}
impl Drop for ModeWorker {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
