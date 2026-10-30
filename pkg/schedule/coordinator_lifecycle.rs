// Copyright 2026 AsterSQL.
//! Cooperative coordinator workers on the host executor.
use super::*;
use astersql_pd_schedule_checker::{Patrol, PatrolError, PatrolHost, SuspectRanges};
use astersql_pd_schedule_hbstream::routing::RoutingClosed;
use futures_util::{FutureExt, StreamExt, future::LocalBoxFuture, stream::FuturesUnordered};
use tokio::time::{Instant, MissedTickBehavior};

pub const PREPARE_CHECK_INTERVAL: Duration = Duration::from_secs(3);
pub const PUSH_OPERATOR_INTERVAL: Duration = Duration::from_millis(500);
pub const SLOW_NODE_INTERVAL: Duration = Duration::from_secs(60);
#[derive(Debug)]
pub enum CoordinatorLifecycleError<E> {
    Restore(crate::SchedulerRestoreError),
    Patrol(PatrolError<E>),
    Routing(RoutingClosed),
    Scheduler(E),
    Panicked,
    Cancelled,
    Admission(astersql_pd_schedule_operator::AdmissionError),
}
/// Scheduler futures wrap existing canonical ScheduleController::run invocations.
/// The factory receives the same child token as all four coordinator workers.
pub type SchedulerWorkers<'a, E> =
    Box<dyn FnOnce(CancellationToken) -> Vec<LocalBoxFuture<'a, Result<(), E>>> + 'a>;
pub struct CoordinatorLifecycleInputs<'a, H: PatrolHost> {
    pub host: &'a H,
    pub suspect_ranges: &'a SuspectRanges,
    pub cancellation: CancellationToken,
    pub elapsed: &'a dyn Fn() -> Duration,
    pub unix_nanos: &'a dyn Fn() -> i128,
    pub push_time: &'a dyn Fn() -> std::time::Instant,
    pub raft_kv2: &'a dyn Fn() -> bool,
    pub cluster_id: u64,
    pub conf_change_v2: bool,
    pub waiting_random: &'a dyn Fn() -> f64,
    pub scheduler_workers: SchedulerWorkers<'a, H::Error>,
}
/// Run returns a host-driven future. Poll wait concurrently with heartbeat routing.
/// Cancel and await wait before destroying host state; dropping is not a join.
#[must_use = "poll wait and cancel cooperatively before shutting down host state"]
pub struct CoordinatorRun<'a, E> {
    cancellation: CancellationToken,
    joined: LocalBoxFuture<'a, Vec<CoordinatorLifecycleError<E>>>,
}
impl<E> CoordinatorRun<'_, E> {
    pub fn stop(&self) {
        self.cancellation.cancel();
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub async fn wait(self) -> Vec<CoordinatorLifecycleError<E>> {
        self.joined.await
    }
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Coordinator<C> {
    pub async fn run<'a, H: PatrolHost + 'a>(
        &'a mut self,
        input: CoordinatorLifecycleInputs<'a, H>,
    ) -> Result<CoordinatorRun<'a, H::Error>, CoordinatorLifecycleError<H::Error>>
    where
        H::Error: 'a,
    {
        let cancel = input.cancellation.child_token();
        loop {
            if cancel.is_cancelled() {
                return Err(CoordinatorLifecycleError::Cancelled);
            }
            if self.should_run((input.elapsed)()) {
                break;
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(CoordinatorLifecycleError::Cancelled),
                _ = tokio::time::sleep(PREPARE_CHECK_INTERVAL) => {}
            }
        }
        if let Err(error) = self.init_schedulers(true, &cancel).await {
            self.schedulers.clean_registered().await;
            return Err(CoordinatorLifecycleError::Restore(error));
        }
        let mut patrol = Patrol::new();
        self.checkers
            .lock()
            .unwrap()
            .set_patrol_status(patrol.subscribe());
        let checkers = &self.checkers;
        let cluster = &*self.cluster;
        let config = &*self.config;
        let operators = &*self.operators;
        let records = &self.records;
        let streams = &self.heartbeat;
        let schedulers = &mut self.schedulers;
        let joined_cancel = cancel.clone();
        let joined = async move {
            let workers: FuturesUnordered<
                LocalBoxFuture<'_, Result<(), CoordinatorLifecycleError<H::Error>>>,
            > = FuturesUnordered::new();
            workers.push(
                async {
                    patrol
                        .run(input.host, joined_cancel.clone())
                        .await
                        .map_err(CoordinatorLifecycleError::Patrol)
                }
                .boxed_local(),
            );
            workers.push(
                input
                    .suspect_ranges
                    .run(
                        || async {
                            input
                                .suspect_ranges
                                .check_once(cluster, &checkers.lock().unwrap());
                            Ok(())
                        },
                        joined_cancel.clone(),
                    )
                    .boxed_local(),
            );
            workers.push(
                async {
                    let dispatch = astersql_pd_schedule_operator::ControllerDispatch::new(
                        operators,
                        cluster,
                        config,
                        streams,
                        input.cluster_id,
                    );
                    let mut known = BTreeMap::new();
                    let mut ticker = timer(PUSH_OPERATOR_INTERVAL);
                    loop {
                        tokio::select! {
                            biased;
                            _ = joined_cancel.cancelled() => return Ok(()),
                            _ = ticker.tick() => {}
                        }
                        let now = (input.push_time)();
                        let nanos = (input.unix_nanos)();
                        // Discover admissions from the same canonical controller, retaining
                        // identity so replacing an operator also schedules its new deadline.
                        let active: BTreeMap<_, _> = operators
                            .operators()
                            .into_iter()
                            .map(|op| (op.region_id(), op))
                            .collect();
                        known.retain(|id, _| active.contains_key(id));
                        for op in active.into_values() {
                            if !known
                                .get(&op.region_id())
                                .is_some_and(|old| Arc::ptr_eq(old, &op))
                            {
                                dispatch.schedule(op.clone(), now);
                                known.insert(op.region_id(), op);
                            }
                        }
                        loop {
                            let poll = dispatch.poll_due(now);
                            if !poll.next {
                                break;
                            }
                            records.bury_removed(&poll.removed, now);
                            let Some(region) = poll.region else {
                                continue;
                            };
                            let out = dispatch
                                .dispatch(
                                    &region,
                                    astersql_pd_schedule_operator::DispatchSource::Notifier,
                                    now,
                                    nanos,
                                    input.conf_change_v2,
                                )
                                .await
                                .map_err(CoordinatorLifecycleError::Routing)?;
                            records.record_dispatch(&out, now);
                            if out.record_non_witness {
                                checkers
                                    .lock()
                                    .unwrap()
                                    .record_region_promote_to_non_witness(
                                        region.meta().id,
                                        nanos,
                                        config.get_switch_witness_interval().as_nanos() as i128,
                                    );
                            }
                            if out.promote_waiting {
                                if let Some(out) = operators
                                    .promote_limited_waiting(
                                        cluster,
                                        config,
                                        (input.waiting_random)(),
                                    )
                                    .await
                                    .map_err(CoordinatorLifecycleError::Admission)?
                                {
                                    records.bury_removed(&out.removed, now);
                                    for op in out.admitted {
                                        dispatch.schedule(op, now);
                                    }
                                }
                            }
                        }
                    }
                }
                .boxed_local(),
            );
            workers.push(
                async {
                    let mut ticker = timer(SLOW_NODE_INTERVAL);
                    let mut registry =
                        astersql_pd_schedule_schedulers::SchedulerRegistry::default();
                    registry
                        .register_builtins()
                        .expect("unique builtin registry");
                    loop {
                        tokio::select! {
                            biased;
                            _ = joined_cancel.cancelled() => return Ok(()),
                            _ = ticker.tick() => {}
                        }
                        if schedulers
                            .exists("evict-slow-trend-scheduler")
                            .unwrap_or(false)
                        {
                            return Ok(());
                        }
                        if (input.raft_kv2)() {
                            // Creation/add failures are nonfatal and retry next minute.
                            if let Ok(policy) = registry.create_args(
                                &astersql_pd_schedule_types::EVICT_SLOW_TREND_SCHEDULER,
                                &[],
                                schedulers.context(),
                            ) {
                                if let Err(error) = schedulers.add(policy, &[], false).await {
                                    schedulers.cleanup_errors.push(error.to_string());
                                }
                            }
                        }
                    }
                }
                .boxed_local(),
            );
            for worker in (input.scheduler_workers)(joined_cancel.clone()) {
                workers.push(
                    async { worker.await.map_err(CoordinatorLifecycleError::Scheduler) }
                        .boxed_local(),
                );
            }
            let mut guarded: FuturesUnordered<_> = workers
                .into_iter()
                .map(|worker| std::panic::AssertUnwindSafe(worker).catch_unwind())
                .collect();
            let mut errors = Vec::new();
            while let Some(result) = guarded.next().await {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        errors.push(error);
                        joined_cancel.cancel();
                    }
                    Err(_) => {
                        errors.push(CoordinatorLifecycleError::Panicked);
                        joined_cancel.cancel();
                    }
                }
            }
            drop(guarded);
            // Cleanup follows the join, preserving prepared state until the last
            // policy/check has finished. Configuration and storage entries remain.
            schedulers.clean_registered().await;
            errors
        }
        .boxed_local();
        Ok(CoordinatorRun {
            cancellation: cancel,
            joined,
        })
    }
    pub async fn run_until_stop<'a, H: PatrolHost + 'a>(
        &'a mut self,
        input: CoordinatorLifecycleInputs<'a, H>,
    ) -> Result<Vec<CoordinatorLifecycleError<H::Error>>, CoordinatorLifecycleError<H::Error>>
    where
        H::Error: 'a,
    {
        Ok(self.run(input).await?.wait().await)
    }
}

fn timer(period: Duration) -> tokio::time::Interval {
    let mut timer = tokio::time::interval_at(Instant::now() + period, period);
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    timer
}
