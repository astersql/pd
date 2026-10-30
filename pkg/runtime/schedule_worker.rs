// Copyright 2026 AsterSQL.
//! One term-scoped coordinator, polled by the supplied executor's blocking pool.
//! Local typed policy futures stay on that task; no runtime or thread is created here.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_hbstream::{
    routing::{RoutingHandle, StreamRouter},
    stream_lifecycle::{HeartbeatMetrics, HeartbeatStreams},
};
use astersql_pd_schedule_operator::{ControllerDispatch, DispatchSource};
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use std::{collections::BTreeMap, time::Duration};

/// Observable term lifecycle, without retaining worker or storage ownership.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchedulePhase {
    #[default]
    Starting,
    Preparing,
    Running,
    Stopped,
}
/// Progress publication lets a host coordinate its clock with completed work.
#[derive(Clone, Debug, Default)]
pub struct ScheduleStatus {
    pub phase: SchedulePhase,
    pub ticks: u64,
}
pub(super) struct ScheduleWorker {
    task: tokio::task::JoinHandle<()>,
    streams: HeartbeatStreams,
}
impl ScheduleWorker {
    pub(super) fn start(
        resources: Arc<AsyncMutex<Option<Resources>>>,
        requests: tokio::sync::mpsc::Receiver<super::schedule_scatter::Command>,
        cluster: BasicCluster,
        configuration: serde_json::Value,
        cancel: CancellationToken,
        error: Arc<Mutex<Option<String>>>,
        initialized: Arc<std::sync::atomic::AtomicBool>,
        cluster_id: u64,
    ) -> (
        RoutingHandle,
        Self,
        tokio::sync::watch::Receiver<ScheduleStatus>,
    ) {
        let (status, status_rx) = tokio::sync::watch::channel(ScheduleStatus::default());
        let runtime = tokio::runtime::Handle::current();
        let (routing, router) = StreamRouter::new(Arc::new(cluster.stores.clone()));
        let streams = HeartbeatStreams::start(
            router,
            cluster_id,
            Arc::new(HeartbeatMetrics::new(&prometheus::Registry::new()).unwrap()),
            &runtime,
        );
        let worker_routing = routing.clone();
        let executor = runtime.clone();
        let task = runtime.spawn_blocking(move || {
            let local = tokio::task::LocalSet::new();
            executor.block_on(local.run_until(async move {
                let result = run(
                    resources,
                    requests,
                    Arc::new(cluster),
                    configuration,
                    worker_routing,
                    cancel,
                    error.clone(),
                    initialized,
                    status.clone(),
                )
                .await;
                status.send_modify(|s| s.phase = SchedulePhase::Stopped);
                if let Err(e) = result {
                    *error.lock().unwrap() = Some(e);
                }
            }));
        });
        (routing, Self { task, streams }, status_rx)
    }
    pub(super) async fn stop(mut self) -> Result<(), tokio::task::JoinError> {
        let result = (&mut self.task).await;
        self.streams.stop();
        let streams = self.streams.join().await;
        result.and(streams)
    }
}
async fn run(
    resources: Arc<AsyncMutex<Option<Resources>>>,
    mut requests: tokio::sync::mpsc::Receiver<super::schedule_scatter::Command>,
    cluster: Arc<BasicCluster>,
    configuration: serde_json::Value,
    routing: RoutingHandle,
    cancel: CancellationToken,
    diagnostics: Arc<Mutex<Option<String>>>,
    initialized: Arc<std::sync::atomic::AtomicBool>,
    status: tokio::sync::watch::Sender<ScheduleStatus>,
) -> Result<(), String> {
    let start = tokio::time::Instant::now();
    let (config, storage, paths) = {
        let guard = resources.lock().await;
        let r = guard.as_ref().ok_or("node stopped")?;
        let config = r.state.lock().unwrap().config.clone();
        (config, r.storage.clone(), r.paths)
    };
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        cancel.clone(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let counts = cluster.clone();
    let mut coordinator = Coordinator::new(CoordinatorInputs {
        cluster: cluster.clone(),
        config: Arc::new(config),
        heartbeat: routing.clone(),
        pending: pending.clone(),
        factory: FactoryContext::new(storage.clone(), paths, JobTime(now().1 as i128)),
        scheduler_configuration: configuration,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new())
            .map_err(|e| e.to_string())?,
        start: Duration::ZERO,
        now_unix_nanos: now().1 as i128,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(move || Ok(counts.regions.total_region_count() as i64)),
        unix_seconds: Arc::new(|| now().0 as u64),
    });
    {
        let checker = Arc::downgrade(coordinator.get_checker_controller());
        let guard = resources.lock().await;
        let r = guard.as_ref().ok_or("node stopped")?;
        r.progress
            .set_patrol_regions_duration_getter(Arc::new(move || {
                checker
                    .upgrade()
                    .map(|c| c.lock().unwrap().patrol_regions_duration())
                    .unwrap_or_default()
            }));
    }
    let labels = Arc::new(
        astersql_pd_schedule_labeler::RegionLabeler::load(
            storage.as_ref().as_ref().clone(),
            paths,
            now().1 as i128,
        )
        .await
        .map_err(|e| e.to_string())?,
    );
    let ranges = astersql_pd_schedule_keyrange::Manager::new();
    let mut ids = std::collections::VecDeque::new();
    let mut hot = astersql_pd_schedule_schedulers::HotLoadProjection::default();
    let mut push_queue = Default::default();
    let mut fast_operators = Default::default();
    let mut splits = Vec::new();
    let result = async {
        status.send_modify(|s| s.phase = SchedulePhase::Preparing);
        while !coordinator.should_run(start.elapsed()) {
            // History sampling is not gated by scheduler preparation.
            let history_wait={
                let mut guard=resources.lock().await;
                let r=guard.as_mut().ok_or("node stopped")?;
                if let Err(e)=super::hot_history::maintain(r,&coordinator,&cancel).await{*diagnostics.lock().unwrap()=Some(e);}
                r.history.wait()
            };
            super::schedule_split::drive(&mut splits, &mut coordinator, &resources).await;
            super::schedule_scatter::dispatch_new(&coordinator,&resources,&routing,&mut push_queue,&mut fast_operators,&cancel).await?;
            tokio::select! {biased;
                _=cancel.cancelled()=>return Ok(()),
                command=requests.recv()=>{if let Some(command)=command {super::schedule_scatter::serve(command,&mut coordinator,&resources,&labels,&cancel,&mut splits).await;
                    super::schedule_scatter::dispatch_new(&coordinator,&resources,&routing,&mut push_queue,&mut fast_operators,&cancel).await?;}},
                _=tokio::time::sleep(history_wait.min(if splits.is_empty() { Duration::from_secs(3) } else { Duration::from_millis(10) }))=>{}
            }
        }
        if cancel.is_cancelled() {
            return Ok(());
        }
        {
            let mut guard = resources.lock().await;
            let r = guard.as_mut().ok_or("node stopped")?;
            r.leader.validate().await.map_err(|e| e.to_string())?;
            // Configuration workers may have committed while the prepare gate waited.
            // Read their published generation under the same admission lock as restore.
            coordinator
                .get_schedulers_controller_mut()
                .set_configuration(
                    serde_json::to_value(r.persisted_config.persistence_snapshot())
                        .map_err(|e| e.to_string())?,
                );
            let report = coordinator
                .init_schedulers(true, &cancel)
                .await
                .map_err(|e| e.to_string())?;
            *diagnostics.lock().unwrap() = report.persistence_error;
            r.persisted_config.config.schedule.schedulers = Some(report.valid_configs.clone());
            r.state.lock().unwrap().config.schedule.schedulers = Some(report.valid_configs);
            initialized.store(true, std::sync::atomic::Ordering::Release);
        }
        let mut intervals = BTreeMap::<String, (tokio::time::Instant, Duration)>::new();
        let mut ticker = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_millis(10),
            Duration::from_millis(10),
        );
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        status.send_modify(|s| s.phase = SchedulePhase::Running);
        let mut last_push = tokio::time::Instant::now();
        let mut last_slow = tokio::time::Instant::now();
        loop {
            super::schedule_split::drive(&mut splits, &mut coordinator, &resources).await;
            super::schedule_scatter::dispatch_new(&coordinator,&resources,&routing,&mut push_queue,&mut fast_operators,&cancel).await?;
            tokio::select! {biased;
                _=cancel.cancelled()=>break,
                _=ticker.tick()=>{},
                command=requests.recv()=>{if let Some(command)=command {super::schedule_scatter::serve(command,&mut coordinator,&resources,&labels,&cancel,&mut splits).await;
                    super::schedule_scatter::dispatch_new(&coordinator,&resources,&routing,&mut push_queue,&mut fast_operators,&cancel).await?;} continue;},
            }
            let mut guard = resources.lock().await;
            let Some(r) = guard.as_mut() else { break };
            r.leader.validate().await.map_err(|e| e.to_string())?;
            if cancel.is_cancelled() {
                break;
            }
            let admission = coordinator.operator_handle();
            let config = r.state.lock().unwrap().config.clone();
            if let Err(e) = super::hot_history::maintain(r, &coordinator, &cancel).await {
                *diagnostics.lock().unwrap() = Some(e);
            }

            let mut dispatch = ControllerDispatch::new(
                &admission,
                &cluster,
                &config,
                &routing,
                paths.cluster_id(),
            );
            dispatch.push_queue = std::mem::take(&mut push_queue);
            dispatch.fast_operators = std::mem::take(&mut fast_operators);
            for id in std::mem::take(&mut r.schedule_observations) {
                if let Some(region) = cluster.regions.get_region(id) {
                    let out = dispatch
                        .dispatch(
                            &region,
                            DispatchSource::Heartbeat,
                            tokio::time::Instant::now().into_std(),
                            now().1 as i128,
                            true,
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    coordinator
                        .get_operator_records()
                        .record_dispatch(&out, tokio::time::Instant::now().into_std());
                }
            }
            if last_push.elapsed() >= Duration::from_millis(500) {
                for out in dispatch
                    .push_due(
                        tokio::time::Instant::now().into_std(),
                        now().1 as i128,
                        true,
                    )
                    .await
                    .map_err(|e| e.to_string())?
                {
                    coordinator
                        .get_operator_records()
                        .record_dispatch(&out, tokio::time::Instant::now().into_std());
                }
                if let Some(promoted) = admission
                    .promote_limited_waiting(&cluster, &config, rand::random())
                    .await
                    .map_err(|e| e.to_string())?
                {
                    coordinator
                        .get_operator_records()
                        .bury_removed(&promoted.removed, tokio::time::Instant::now().into_std());
                    for op in promoted.admitted {
                        dispatch.schedule(op.clone(), tokio::time::Instant::now().into_std());
                        if let Some(region) = cluster.regions.get_region(op.region_id()) {
                            let out = dispatch
                                .dispatch(
                                    &region,
                                    DispatchSource::Create,
                                    tokio::time::Instant::now().into_std(),
                                    now().1 as i128,
                                    true,
                                )
                                .await
                                .map_err(|e| e.to_string())?;
                            coordinator
                                .get_operator_records()
                                .record_dispatch(&out, tokio::time::Instant::now().into_std());
                        }
                    }
                }
                last_push = tokio::time::Instant::now();
            }
            push_queue = dispatch.push_queue;
            fast_operators = dispatch.fast_operators;
            if last_slow.elapsed() >= Duration::from_secs(60) {
                let controller = coordinator.get_schedulers_controller_mut();
                if config.store.is_raft_kv2()
                    && !controller
                        .exists("evict-slow-trend-scheduler")
                        .unwrap_or(false)
                {
                    controller.set_configuration(
                        serde_json::to_value(r.persisted_config.persistence_snapshot())
                            .map_err(|e| e.to_string())?,
                    );
                    let mut registry =
                        astersql_pd_schedule_schedulers::SchedulerRegistry::default();
                    registry.register_builtins().map_err(|e| e.to_string())?;
                    let policy = registry.create_args(
                        &"evict-slow-trend-scheduler".into(),
                        &[],
                        controller.context(),
                    );
                    // Policy construction/registration failures retry at the next minute.
                    match policy {
                        Ok(policy) => {
                            if let Err(e) = controller.add(policy, &[], false).await {
                                *diagnostics.lock().unwrap() = Some(e.to_string())
                            } else {
                                if let Err(e) = controller
                                    .context()
                                    .storage
                                    .save_config(&paths, controller.configuration())
                                    .await
                                {
                                    *diagnostics.lock().unwrap() = Some(e.to_string());
                                }
                                let configs = serde_json::from_value(
                                    controller.configuration()["schedule"]["schedulers-v2"].clone(),
                                )
                                .map_err(|e| e.to_string())?;
                                r.persisted_config.config.schedule.schedulers = Some(configs);
                                r.state.lock().unwrap().config.schedule.schedulers =
                                    r.persisted_config.config.schedule.schedulers.clone();
                            }
                        }
                        Err(e) => *diagnostics.lock().unwrap() = Some(e.to_string()),
                    }
                }
                last_slow = tokio::time::Instant::now();
            }
            let names = coordinator.get_schedulers_controller().names();
            for name in names {
                let current = tokio::time::Instant::now();
                let (due, interval) = intervals
                    .entry(name.clone())
                    .or_insert((current, Duration::from_millis(10)));
                if current < *due {
                    continue;
                }
                let config = r.state.lock().unwrap().config.clone();
                if config.schedule.halt_scheduling
                    || coordinator
                        .get_schedulers_controller()
                        .is_paused(&name, now().0)
                        .unwrap_or(true)
                {
                    continue;
                }
                let policy = &mut coordinator
                    .get_schedulers_controller_mut()
                    .get_mut(&name)
                    .ok_or("scheduler disappeared")?
                    .scheduler
                    .policy;
                if !super::schedule_policy::allowed(policy, &config, &admission) {
                    continue;
                }
                if ids.len() < 64 {
                    let (last, count) = r
                        .allocator
                        .alloc_count(128)
                        .await
                        .map_err(|e| e.to_string())?;
                    ids.extend(last - u64::from(count) + 1..=last);
                }
                let stats = r
                    .hot
                    .snapshot(
                        0,
                        cluster
                            .regions
                            .get_regions()
                            .iter()
                            .map(|r| r.meta().id)
                            .collect(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                super::schedule_scheduler::observe_hot_support(policy,&r.persisted_config.config.cluster_version)?;
                let mut state = r.state.lock().unwrap();
                let mut inputs = super::schedule_policy::PolicyInputs {
                    cluster: &cluster,
                    config: &config,
                    rules: &mut state.rules,
                    statistics: &stats,
                    labels: &labels,
                    ranges: &ranges,
                    admission: &admission,
                    fast: &fast_operators,
                    storage: &r.storage,
                    paths: r.paths,
                    ids: &mut ids,
                    hot: &mut hot,
                };
                let output = inputs.schedule(policy, interval, &cancel).await;
                let output = match output {
                    Ok(output) => output,
                    Err(e) => {
                        *diagnostics.lock().unwrap() = Some(format!("{name}: {e}"));
                        *interval = (*interval * 2).min(Duration::from_secs(5));
                        *due = current + *interval;
                        continue;
                    }
                };
                drop(inputs);
                drop(state);
                *due = current + *interval;
                let mut index = 0;
                while index < output.len() {
                    let width = if output[index].additional_info().has_related_merge_region() {
                        2
                    } else {
                        1
                    };
                    let end = (index + width).min(output.len());
                    let admitted = admission.enqueue_waiting(&cluster, &output[index..end]);
                    index = end;
                    if admitted.is_err() {
                        continue;
                    }
                    let promoted = admission
                        .promote_limited_waiting(&cluster, &config, rand::random::<f64>())
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(promoted) = promoted {
                        let mut dispatch = ControllerDispatch::new(
                            &admission,
                            &cluster,
                            &config,
                            &routing,
                            paths.cluster_id(),
                        );
                        dispatch.push_queue = std::mem::take(&mut push_queue);
                        dispatch.fast_operators = std::mem::take(&mut fast_operators);
                        for op in promoted.admitted {
                            dispatch.schedule(op.clone(), tokio::time::Instant::now().into_std());
                            let Some(region) = cluster.regions.get_region(op.region_id()) else {
                                continue;
                            };
                            let out = dispatch
                                .dispatch(
                                    &region,
                                    DispatchSource::Create,
                                    tokio::time::Instant::now().into_std(),
                                    now().1 as i128,
                                    true,
                                )
                                .await
                                .map_err(|e| e.to_string())?;
                            coordinator
                                .get_operator_records()
                                .record_dispatch(&out, tokio::time::Instant::now().into_std());
                        }
                        push_queue = dispatch.push_queue;
                        fast_operators = dispatch.fast_operators;
                    }
                }
            }
            status.send_modify(|s| s.ticks = s.ticks.saturating_add(1));
        }
        Ok(())
    }
    .await;
    coordinator
        .get_schedulers_controller_mut()
        .clean_registered()
        .await;
    pending.shutdown().await;
    result
}

/// Bind transport sends to a term even when the node's listener survives re-election.
pub(super) struct TermStream {
    pub(super) inner: Arc<dyn astersql_pd_schedule_hbstream::routing::HeartbeatStream>,
    pub(super) cancel: CancellationToken,
}
impl astersql_pd_schedule_hbstream::routing::HeartbeatStream for TermStream {
    fn send(
        &self,
        command: astersql_pd_schedule_hbstream::HeartbeatCommand,
    ) -> astersql_pd_schedule_hbstream::routing::StreamSend<'_> {
        Box::pin(async move {
            tokio::select! {biased;
                _=self.cancel.cancelled()=>Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe,"leader term stopped").into()),
                result=self.inner.send(command)=>result,
            }
        })
    }
    fn keep_alive(
        &self,
        cluster_id: u64,
    ) -> astersql_pd_schedule_hbstream::routing::StreamSend<'_> {
        Box::pin(async move {
            tokio::select! {biased;
                _=self.cancel.cancelled()=>Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe,"leader term stopped").into()),
                result=self.inner.keep_alive(cluster_id)=>result,
            }
        })
    }
}
