// Copyright 2026 AsterSQL.
//! Patrol lifecycle on the host executor, with joined, cooperatively canceled workers.
use astersql_pd_core::RegionInfo;
use async_trait::async_trait;
use futures_util::{FutureExt, StreamExt, future::LocalBoxFuture, stream::FuturesUnordered};
use std::{future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, mpsc, watch},
    time::{Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

pub const PATROL_CHANNEL_CAPACITY: usize = 8192;
/// Matches calculateScanLimit, including truncation before clamping.
pub fn patrol_scan_limit(regions: usize) -> usize {
    (regions / 1024).clamp(128, PATROL_CHANNEL_CAPACITY)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatrolConfig {
    pub interval: Duration,
    pub workers: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatrolSource {
    Priority,
    Pending,
    Scan,
}
/// Host-owned state access. Implementations use the canonical registry, dispatcher,
/// split-scatter controller and statistics; futures need not be Send because the
/// dispatcher borrows local rule state. Run on the host task/LocalSet, never a new runtime.
#[async_trait(?Send)]
pub trait PatrolHost {
    type Error;
    async fn config(&self) -> PatrolConfig;
    async fn region_count(&self) -> usize;
    async fn scheduling_halted(&self) -> bool;
    async fn priority_ids(&self) -> Vec<u64>;
    async fn pending_ids(&self) -> Vec<u64>;
    async fn region(&self, id: u64) -> Option<Arc<RegionInfo>>;
    async fn remove_priority(&self, id: u64);
    async fn check(&self, region: Arc<RegionInfo>, source: PatrolSource)
    -> Result<(), Self::Error>;
    async fn dispatch_split_scatter(&self);
    async fn clear_split_scatter(&self);
    async fn scan(&self, start: &[u8], limit: usize) -> Vec<Arc<RegionInfo>>;
    async fn update_label_stats(&self, regions: &[Arc<RegionInfo>]);
}
#[derive(Debug, PartialEq, Eq)]
pub enum PatrolError<E> {
    InvalidInterval,
    Host(E),
}
/// Observable state, also available after shutdown. Duration resets on cancellation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PatrolStatus {
    pub cursor: Vec<u8>,
    pub scan_limit: usize,
    pub workers: usize,
    pub interval: Duration,
    pub last_round: Duration,
    pub rounds: u64,
    pub ticks: u64,
}
/// One patrol invocation owns its channel and worker futures. Dropping the run
/// future is not graceful shutdown: cancel the supplied token and await run instead.
pub struct Patrol {
    metrics: Option<crate::CheckerMetrics>,
    status: watch::Sender<PatrolStatus>,
}
impl Default for Patrol {
    fn default() -> Self {
        Self::new()
    }
}
impl Patrol {
    pub fn new() -> Self {
        Self {
            metrics: None,
            status: watch::channel(PatrolStatus::default()).0,
        }
    }
    pub fn set_metrics(&mut self, metrics: crate::CheckerMetrics) {
        self.metrics = Some(metrics);
    }
    pub fn subscribe(&self) -> watch::Receiver<PatrolStatus> {
        self.status.subscribe()
    }
    pub async fn run<H: PatrolHost>(
        &mut self,
        host: &H,
        cancel: CancellationToken,
    ) -> Result<(), PatrolError<H::Error>> {
        let metrics = self.metrics.clone();
        let initial = host.config().await;
        if initial.interval.is_zero() {
            return Err(PatrolError::InvalidInterval);
        }
        let (tx, rx) = mpsc::channel(PATROL_CHANNEL_CAPACITY);
        let rx = Mutex::new(rx);
        let mut workers: Workers<'_, H::Error> = FuturesUnordered::new();
        let mut worker_cancel = CancellationToken::new();
        // Go initially has zero workers and starts configured workers on the first tick.
        let mut state = PatrolStatus {
            interval: initial.interval,
            scan_limit: patrol_scan_limit(host.region_count().await),
            ..Default::default()
        };
        self.status.send_replace(state.clone());
        let mut ticker =
            tokio::time::interval_at(Instant::now() + initial.interval, initial.interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut start = Instant::now();
        let result = async {
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Ok(()),
                    Some(result) = workers.next(), if !workers.is_empty() => { result.map_err(PatrolError::Host)?; continue; },
                    _ = ticker.tick() => {}
                }
                if cancel.is_cancelled() { return Ok(()); }
                state.ticks += 1;
                let config = host.config().await;
                if config.interval.is_zero() { return Err(PatrolError::InvalidInterval); }
                if config.interval != state.interval {
                    state.interval = config.interval;
                    ticker.reset_after(config.interval);
                }
                if config.workers != state.workers {
                    worker_cancel.cancel();
                    drain(&mut workers).await.map_err(PatrolError::Host)?;
                    worker_cancel = CancellationToken::new();
                    state.workers = config.workers;
                    for _ in 0..config.workers {
                        workers.push(worker(host, &rx, worker_cancel.clone()).boxed_local());
                    }
                }
                let tick = async {
                    if host.scheduling_halted().await {
                        if let Ok(mut receiver) = rx.try_lock() {
                            while receiver.try_recv().is_ok() {}
                        }
                        return Ok(());
                    }
                    // Do not advance the cursor or recheck lists while backlog remains.
                    if let Some(m) = &metrics { m.inner.channel_size.set((PATROL_CHANNEL_CAPACITY - tx.capacity()) as f64); }
                    if tx.capacity() < PATROL_CHANNEL_CAPACITY {
                        let _timer = metrics.as_ref().map(|m| m.phase_metrics("wait_for_channel"));
                        return Ok(());
                    }
                    {
                    let _timer = metrics.as_ref().map(|m| m.phase_metrics("check_priority"));
                    let ids = host.priority_ids().await;
                    if let Some(m) = &metrics { m.queue_metrics(true, ids.len()); }
                    for id in ids {
                        if let Some(region) = host.region(id).await {
                            host.check(region, PatrolSource::Priority).await?;
                        } else { host.remove_priority(id).await; }
                    }
                    }
                    {
                    let _timer = metrics.as_ref().map(|m| m.phase_metrics("check_pending"));
                    let ids = host.pending_ids().await;
                    if let Some(m) = &metrics { m.queue_metrics(false, ids.len()); }
                    for id in ids {
                        if let Some(region) = host.region(id).await {
                            host.check(region, PatrolSource::Pending).await?;
                        }
                    }
                    }
                    {
                    let _timer = metrics.as_ref().map(|m| m.phase_metrics("dispatch_split_scatter"));
                    host.dispatch_split_scatter().await;
                    }
                    let scan_timer = metrics.as_ref().map(|m| m.phase_metrics("scan_regions"));
                    let regions = host.scan(&state.cursor, state.scan_limit).await;
                    if regions.is_empty() { state.cursor.clear(); return Ok(()); }
                    for region in &regions {
                        state.cursor = region.meta().key_range.end_key.clone();
                        // Bounded scan <= channel capacity; shutdown also handles a blocked send.
                        tokio::select! {
                            biased;
                            _ = cancel.cancelled() => return Ok(()),
                            result = tx.send(region.clone()) => { if result.is_err() { return Ok(()); } }
                        }
                    }
                    drop(scan_timer);
                    {
                    let _timer = metrics.as_ref().map(|m| m.phase_metrics("update_label_stats"));
                    host.update_label_stats(&regions).await;
                    }
                    if state.cursor.is_empty() {
                        state.scan_limit = patrol_scan_limit(host.region_count().await);
                        state.last_round = start.elapsed();
                        if let Some(m) = &metrics { m.inner.patrol_time.set(state.last_round.as_secs_f64()); }
                        state.rounds += 1;
                        start = Instant::now();
                    }
                    Ok(())
                };
                drive(tick, &mut workers).await.map_err(PatrolError::Host)?;
                self.status.send_replace(state.clone());
            }
        }.await;
        drop(tx);
        worker_cancel.cancel();
        let joined = drain(&mut workers).await.map_err(PatrolError::Host);
        // Go defer order waits for workers before clearing split-scatter state.
        host.clear_split_scatter().await;
        state.workers = 0;
        state.last_round = Duration::ZERO;
        if let Some(m) = &metrics {
            m.inner.patrol_time.set(0.0);
        }
        self.status.send_replace(state);
        result.and(joined)
    }
}
type Workers<'a, E> = FuturesUnordered<LocalBoxFuture<'a, Result<(), E>>>;
async fn worker<H: PatrolHost>(
    host: &H,
    rx: &Mutex<mpsc::Receiver<Arc<RegionInfo>>>,
    cancel: CancellationToken,
) -> Result<(), H::Error> {
    loop {
        let region = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            region = async { rx.lock().await.recv().await } => region,
        };
        let Some(region) = region else {
            return Ok(());
        };
        // In-flight checks finish even after cancellation; no detached work survives shutdown.
        host.check(region, PatrolSource::Scan).await?;
    }
}
async fn drain<E>(workers: &mut Workers<'_, E>) -> Result<(), E> {
    let mut error = None;
    while let Some(result) = workers.next().await {
        if let Err(e) = result {
            if error.is_none() {
                error = Some(e);
            }
        }
    }
    error.map_or(Ok(()), Err)
}
async fn drive<E>(
    future: impl Future<Output = Result<(), E>>,
    workers: &mut Workers<'_, E>,
) -> Result<(), E> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result,
            Some(result) = workers.next(), if !workers.is_empty() => result?,
        }
    }
}

impl crate::CheckerDispatcher {
    /// Canonical patrol dispatch. Priority checks skip merge pairs and preserve
    /// pending state; pending/scan checks use try_add_operators transitions.
    pub async fn check_patrol_region<C, F>(
        &mut self,
        ctx: &crate::CheckerDispatchContext<'_, C>,
        rules: &mut astersql_pd_schedule_placement::RuleManager,
        affinity: &mut astersql_pd_schedule_affinity::manager::Manager,
        region: &Arc<RegionInfo>,
        source: PatrolSource,
        allocate: &mut F,
    ) -> crate::TryAddCheckersOutcome
    where
        C: astersql_pd_schedule_config::CheckerConfigProvider,
        F: FnMut() -> Result<u64, astersql_pd_schedule_operator::BuilderError>,
    {
        if source != PatrolSource::Priority {
            return self
                .try_add_operators(ctx, rules, affinity, Some(region), allocate)
                .await;
        }
        let dispatch = self
            .check_region(ctx, rules, affinity, region, allocate)
            .await;
        let mut result = crate::TryAddCheckersOutcome {
            errors: dispatch.errors,
            ..Default::default()
        };
        if dispatch.operators.is_empty()
            || dispatch.operators[0]
                .additional_info()
                .has_related_merge_region()
        {
            return result;
        }
        let ops: Vec<_> = dispatch.operators.into_iter().map(Arc::new).collect();
        if astersql_pd_schedule_operator::exceed_store_limit(
            &ops,
            ctx.cluster,
            ctx.affinity.rules.replica.config,
        )
        .await
        {
            result.store_limited = true;
        } else {
            result.admission_error = ctx.operators.enqueue_waiting(ctx.cluster, &ops).err();
            if result.admission_error.is_none() {
                result.queued = ops;
            }
        }
        result
    }
}
