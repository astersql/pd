// Copyright 2026 AsterSQL.
//! Host-runtime scheduling, immutable store-list views and cooperative shutdown.
use crate::{IntervalGrowth, MAX_SCHEDULE_INTERVAL, MIN_SCHEDULE_INTERVAL, interval_grow};
use astersql_pd_core::{
    BasicCluster as CoreCluster, RegionInfo, RegionsInfo, StoreInfo, StoresInfo,
};
use astersql_pd_schedule_core::BasicCluster;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{AdmissionError, ControllerAdmission, Operator};
use futures_util::FutureExt;
use std::panic::{AssertUnwindSafe, resume_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, Ordering},
};
use tokio::{
    task::JoinHandle,
    time::{Duration, Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

/// Only GetStores is frozen per attempt; region and individual-store reads stay live.
pub struct CacheCluster<'a> {
    pub cluster: &'a CoreCluster,
    stores: Vec<Arc<StoreInfo>>,
}
impl<'a> CacheCluster<'a> {
    pub fn new(cluster: &'a CoreCluster) -> Self {
        Self {
            cluster,
            stores: cluster.stores.get_stores(),
        }
    }
}
impl BasicCluster for CacheCluster<'_> {
    fn stores(&self) -> &StoresInfo {
        &self.cluster.stores
    }
    fn regions(&self) -> &RegionsInfo {
        &self.cluster.regions
    }
    fn get_stores(&self) -> Vec<Arc<StoreInfo>> {
        self.stores.clone()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleGate {
    Allowed,
    Pending,
    Halted,
    Paused,
}
/// Shared controls may be updated while the policy is executing. Cancellation is
/// cooperative: an in-flight policy completes before cleanup, matching Go.
#[derive(Clone, Default)]
pub struct LoopControl {
    pub cancellation: CancellationToken,
    halted: Arc<AtomicBool>,
    delay_until: Arc<AtomicI64>,
}
impl LoopControl {
    pub fn from_parent(parent: &CancellationToken) -> Self {
        Self {
            cancellation: parent.child_token(),
            ..Self::default()
        }
    }

    pub fn stop(&self) {
        self.cancellation.cancel();
    }
    pub fn halt(&self, halted: bool) {
        self.halted.store(halted, Ordering::SeqCst);
    }
    pub fn set_delay_until(&self, until: i64) {
        self.delay_until.store(until, Ordering::SeqCst);
    }
    pub fn gate(&self, allowed: bool, now: i64) -> ScheduleGate {
        if !allowed {
            ScheduleGate::Pending
        } else if self.halted.load(Ordering::SeqCst) {
            ScheduleGate::Halted
        } else if now < self.delay_until.load(Ordering::SeqCst) {
            ScheduleGate::Paused
        } else {
            ScheduleGate::Allowed
        }
    }
}
/// Policies retain typed contexts and errors. Implementations call canonical
/// schedulers; this contract does not supply a fallback scheduling policy.
#[allow(async_fn_in_trait)]
pub trait LoopScheduler {
    type Error;
    type Config: astersql_pd_schedule_config::SharedConfigProvider;
    fn admission_config(&self) -> &Self::Config;
    fn waiting_random(&mut self) -> f64;
    fn min_interval(&self) -> i64 {
        MIN_SCHEDULE_INTERVAL
    }
    fn next_interval(&self, current: i64) -> i64 {
        interval_grow(current, MAX_SCHEDULE_INTERVAL, IntervalGrowth::Exponential)
    }
    fn allowed(&self) -> bool;
    fn evict_leader(&self) -> bool {
        false
    }
    async fn schedule(
        &mut self,
        cluster: &CacheCluster<'_>,
        diagnostic: bool,
    ) -> Result<Vec<Arc<Operator>>, Self::Error>;
    async fn clean(&mut self, cluster: &CoreCluster) -> Result<(), Self::Error>;
}
/// Clock/labeler are host-owned boundaries; no private runtime or global state.
pub struct ScheduleController<S> {
    pub scheduler: S,
    pub cluster: Arc<CoreCluster>,
    pub control: LoopControl,
    pub labeler: Option<Arc<RegionLabeler>>,
    pub now_nanos: fn() -> i64,
    interval: i64,
}
impl<S: LoopScheduler> ScheduleController<S> {
    pub fn new(
        scheduler: S,
        cluster: Arc<CoreCluster>,
        control: LoopControl,
        now_nanos: fn() -> i64,
    ) -> Self {
        let interval = scheduler.min_interval();
        Self {
            scheduler,
            cluster,
            control,
            labeler: None,
            now_nanos,
            interval,
        }
    }
    pub fn interval(&self) -> i64 {
        self.interval
    }
    pub fn set_interval(&mut self, interval: i64) {
        self.interval = interval;
    }
    pub fn allow_schedule(&self) -> ScheduleGate {
        self.control
            .gate(self.scheduler.allowed(), (self.now_nanos)() / 1_000_000_000)
    }
    pub async fn schedule(&mut self, diagnostic: bool) -> Result<Vec<Arc<Operator>>, S::Error> {
        'retry: for attempt in 0..10 {
            if self.control.cancellation.is_cancelled() {
                return Ok(vec![]);
            }
            let cache = CacheCluster::new(&self.cluster);
            let mut ops = self
                .scheduler
                .schedule(&cache, diagnostic && attempt == 0)
                .await?;
            if ops.is_empty() {
                continue;
            }
            self.interval = self.scheduler.min_interval();
            let mut i = 0;
            while i < ops.len() {
                let Some(region) = self.cluster.regions.get_region(ops[i].region_id()) else {
                    continue 'retry;
                };
                if !self.scheduler.evict_leader()
                    && self
                        .labeler
                        .as_ref()
                        .is_some_and(|l| l.schedule_disabled(&region, (self.now_nanos)().into()))
                {
                    ops.remove(i);
                } else {
                    i += 1;
                }
            }
            if !ops.is_empty() {
                return Ok(ops);
            }
        }
        self.interval = self.scheduler.next_interval(self.interval);
        Ok(vec![])
    }
    /// Runs on the host's runtime. Errors are retained and clean always executes
    /// before returning. Timer admission failures do not terminate the policy.
    pub async fn run(mut self, admission: Arc<ControllerAdmission>) -> LoopExit<S> {
        let mut errors = vec![];
        let mut admission_errors = vec![];
        if self.interval <= 0 {
            let cleanup = self.scheduler.clean(&self.cluster).await;
            return LoopExit {
                controller: self,
                errors,
                admission_errors,
                cleanup,
                invalid_interval: true,
            };
        }
        let period = Duration::from_nanos(self.interval as u64);
        let mut timer = tokio::time::interval_at(Instant::now() + period, period);
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let execution = AssertUnwindSafe(async {
            loop {
                tokio::select! {
                    biased;
                    _ = self.control.cancellation.cancelled() => break,
                    _ = timer.tick() => {}
                }
                if self.allow_schedule() != ScheduleGate::Allowed {
                    continue;
                }
                match self.schedule(false).await {
                    Ok(ops) if !ops.is_empty() => {
                        // A batch contains independent operators or a reciprocal merge pair.
                        let mut i = 0;
                        let mut need_promoted = 0;
                        while i < ops.len() {
                            let width = if ops[i].additional_info().has_related_merge_region() {
                                2
                            } else {
                                1
                            };
                            let end = (i + width).min(ops.len());
                            if let Err(e) = admission.enqueue_waiting(&self.cluster, &ops[i..end]) {
                                admission_errors.push(e);
                            } else {
                                need_promoted += 1;
                            }
                            i = end;
                        }
                        let draw = self.scheduler.waiting_random();
                        for _ in 0..need_promoted {
                            match admission
                                .promote_limited_waiting(
                                    &self.cluster,
                                    self.scheduler.admission_config(),
                                    draw,
                                )
                                .await
                            {
                                Ok(Some(_)) | Ok(None) => {}
                                Err(e) => {
                                    admission_errors.push(e);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        errors.push(error);
                        break;
                    }
                }
                if self.interval <= 0 {
                    break;
                }
                let period = Duration::from_nanos(self.interval as u64);
                timer = tokio::time::interval_at(Instant::now() + period, period);
                timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
            }
        })
        .catch_unwind()
        .await;
        let invalid_interval = self.interval <= 0;
        let cleanup = self.scheduler.clean(&self.cluster).await;
        if let Err(payload) = execution {
            resume_unwind(payload);
        }
        LoopExit {
            controller: self,
            errors,
            admission_errors,
            cleanup,
            invalid_interval,
        }
    }
    /// Concrete policies contain non-Send typed context borrows. Hosts run them
    /// on their existing LocalSet; this creates neither a runtime nor a thread.
    pub fn start_local(self, admission: Arc<ControllerAdmission>) -> ScheduleTask<S>
    where
        S: 'static,
        S::Error: 'static,
    {
        let control = self.control.clone();
        ScheduleTask {
            control,
            task: tokio::task::spawn_local(self.run(admission)),
        }
    }
}
pub struct LoopExit<S: LoopScheduler> {
    pub controller: ScheduleController<S>,
    pub errors: Vec<S::Error>,
    pub admission_errors: Vec<AdmissionError>,
    pub cleanup: Result<(), S::Error>,
    pub invalid_interval: bool,
}
/// Dropping a handle requests cancellation; wait consumes it and joins cleanup.
pub struct ScheduleTask<S: LoopScheduler> {
    pub control: LoopControl,
    task: JoinHandle<LoopExit<S>>,
}
impl<S: LoopScheduler> ScheduleTask<S> {
    pub fn stop(&self) {
        self.control.stop();
    }
    pub async fn wait(mut self) -> Result<LoopExit<S>, tokio::task::JoinError> {
        (&mut self.task).await
    }
}
impl<S: LoopScheduler> Drop for ScheduleTask<S> {
    fn drop(&mut self) {
        self.control.stop();
    }
}
/// Go's heartbeat eligibility and nonblocking queue delivery. False means an
/// ordinary heartbeat or a full/disconnected queue; no event is replayed.
pub fn send_witness_event(
    sender: &std::sync::mpsc::SyncSender<Arc<RegionInfo>>,
    region: Arc<RegionInfo>,
) -> bool {
    region.leader().is_some_and(|p| p.is_witness) && sender.try_send(region).is_ok()
}

/// Canonical witness scheduler with placement/configuration and explicit clock/random inputs.
pub struct WitnessLoopScheduler {
    pub policy: crate::TransferWitnessLeaderScheduler,
    pub config: astersql_pd_schedule_config::ProviderSnapshot<u64>,
    pub rules: astersql_pd_schedule_placement::RuleManager,
    pub now_nanos: fn() -> i64,
    pub random_index: fn(usize) -> usize,
    pub faults: Vec<crate::TransferWitnessLeaderError>,
}
impl LoopScheduler for WitnessLoopScheduler {
    type Error = crate::TransferWitnessLeaderError;
    type Config = astersql_pd_schedule_config::ProviderSnapshot<u64>;
    fn admission_config(&self) -> &Self::Config {
        &self.config
    }
    fn waiting_random(&mut self) -> f64 {
        (self.random_index)(1 << 24) as f64 / (1 << 24) as f64
    }

    fn allowed(&self) -> bool {
        self.policy.is_schedule_allowed()
    }
    async fn schedule(
        &mut self,
        cluster: &CacheCluster<'_>,
        _: bool,
    ) -> Result<Vec<Arc<Operator>>, Self::Error> {
        let mut random = self.random_index;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = crate::EvictLeaderContext {
            cluster,
            config: &self.config,
            rules: &mut self.rules,
            now_unix_nanos: (self.now_nanos)().into(),
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let out = self.policy.schedule(&mut ctx).await;
        // Go logs and consumes bad events, continuing with good batch members.
        // Keep faults available through the policy API; the loop receives valid ops.
        self.faults.extend(out.errors);
        Ok(out.operators)
    }
    async fn clean(&mut self, _: &CoreCluster) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Eviction uses the same prepared instance; run owns its eventual clean call.
pub struct EvictLoopScheduler {
    pub policy: crate::EvictLeaderScheduler,
    pub config: astersql_pd_schedule_config::ProviderSnapshot<u64>,
    pub rules: astersql_pd_schedule_placement::RuleManager,
    pub now_nanos: fn() -> i64,
    pub random_index: fn(usize) -> usize,
    pub admission: Arc<ControllerAdmission>,
    pub faults: Vec<astersql_pd_schedule_operator::BuilderError>,
}
impl LoopScheduler for EvictLoopScheduler {
    type Error = astersql_pd_core::StoreControlError;
    type Config = astersql_pd_schedule_config::ProviderSnapshot<u64>;
    fn admission_config(&self) -> &Self::Config {
        &self.config
    }
    fn waiting_random(&mut self) -> f64 {
        (self.random_index)(1 << 24) as f64 / (1 << 24) as f64
    }

    fn allowed(&self) -> bool {
        use astersql_pd_schedule_config::SchedulerConfigProvider;
        crate::EvictLeaderScheduler::is_schedule_allowed(
            self.admission
                .operator_count(astersql_pd_schedule_operator::OpKind::LEADER),
            self.config.get_leader_schedule_limit(),
        )
    }
    fn evict_leader(&self) -> bool {
        true
    }
    async fn schedule(
        &mut self,
        cluster: &CacheCluster<'_>,
        _: bool,
    ) -> Result<Vec<Arc<Operator>>, Self::Error> {
        let mut random = self.random_index;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = crate::EvictLeaderContext {
            cluster,
            config: &self.config,
            rules: &mut self.rules,
            now_unix_nanos: (self.now_nanos)().into(),
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let out = self.policy.schedule(&mut ctx).await;
        self.faults.extend(out.errors);
        Ok(out.operators)
    }
    async fn clean(&mut self, cluster: &CoreCluster) -> Result<(), Self::Error> {
        self.policy.clean(cluster);
        Ok(())
    }
}
