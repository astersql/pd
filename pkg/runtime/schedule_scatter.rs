// Copyright 2026 AsterSQL.
//! Requests are serialized by the existing term coordinator, including its prepare gate.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_operator::BuilderError;
use astersql_pd_schedule_scatter::{ScatterBatchError, ScatterBatchFailure, ScatterBatchRequest};
use astersql_pd_server::grpc_scatter::*;
use tokio::sync::oneshot;
pub(super) enum Command {
    Scatter(
        ScatterRequest,
        oneshot::Sender<Result<ScatterResult, ScatterError>>,
    ),
    Operator(u64, oneshot::Sender<Result<Option<OperatorResult>, String>>),
}
impl ClusterHost {
    pub(crate) async fn scatter(
        &self,
        request: ScatterRequest,
    ) -> Result<ScatterResult, ScatterError> {
        let (tx, rx) = oneshot::channel();
        tokio::select! {biased;
            _=self.cancel.cancelled()=>Err(ScatterError::Other("server not started".into())),
            result=async {self.schedule_requests.send(Command::Scatter(request,tx)).await.map_err(|_|ScatterError::Other("scheduling stopped".into()))?;rx.await.map_err(|_|ScatterError::Other("scheduling stopped".into()))?}=>result,
        }
    }
    pub(crate) async fn operator(&self, id: u64) -> Result<Option<OperatorResult>, String> {
        let (tx, rx) = oneshot::channel();
        tokio::select! {biased;
            _=self.cancel.cancelled()=>Err("server not started".into()),
            result=async {self.schedule_requests.send(Command::Operator(id,tx)).await.map_err(|_|"scheduling stopped")?;rx.await.map_err(|_|"scheduling stopped")?}=>result,
        }
    }
}
pub(super) async fn serve(
    command: Command,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    labels: &astersql_pd_schedule_labeler::RegionLabeler,
    cancel: &CancellationToken,
) {
    match command {
        Command::Operator(id, mut reply) => {
            let future = async {
                let guard = resources.lock().await;
                let r = guard.as_ref().ok_or("server not started")?;
                r.leader.validate().await.map_err(|e| e.to_string())?;
                Ok(coordinator
                    .get_operator_records()
                    .get_status(
                        coordinator.get_operator_controller(),
                        id,
                        tokio::time::Instant::now().into_std(),
                    )
                    .map(|record| OperatorResult {
                        desc: record.operator.desc().as_bytes().to_vec(),
                        kind: record.operator.kind().to_string().into_bytes(),
                        status: record.status.0,
                    }))
            };
            tokio::select! {biased;_=cancel.cancelled()=>{},_=reply.closed()=>{},result=future=>{let _=reply.send(result);}}
        }
        Command::Scatter(request, mut reply) => {
            let admission = coordinator.operator_handle();
            let before: std::collections::BTreeMap<_, _> = admission
                .operators()
                .into_iter()
                .map(|op| (op.region_id(), op))
                .collect();
            let result = tokio::select! {biased;_=cancel.cancelled()=>None,_=reply.closed()=>None,result=scatter(request,coordinator,resources,labels,cancel)=>Some(result)};
            // Completed admissions survive caller cancellation. Retain replaced records
            // and queue only new identities, even for partially completed batches.
            for (&id, op) in &before {
                if admission
                    .get_operator(id)
                    .is_none_or(|current| !Arc::ptr_eq(op, &current))
                {
                    coordinator
                        .get_operator_records()
                        .bury_operator(op.clone(), tokio::time::Instant::now().into_std());
                }
            }
            if !cancel.is_cancelled() {
                let mut guard = resources.lock().await;
                if let Some(r) = guard.as_mut() {
                    r.schedule_creations.extend(
                        admission
                            .operators()
                            .into_iter()
                            .filter(|op| {
                                before
                                    .get(&op.region_id())
                                    .is_none_or(|old| !Arc::ptr_eq(old, op))
                            })
                            .map(|op| op.region_id()),
                    );
                }
            }
            if let Some(result) = result {
                let _ = reply.send(result);
            }
        }
    }
}
async fn scatter(
    request: ScatterRequest,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    labels: &astersql_pd_schedule_labeler::RegionLabeler,
    cancel: &CancellationToken,
) -> Result<ScatterResult, ScatterError> {
    let mut guard = resources.lock().await;
    let r = guard
        .as_mut()
        .ok_or_else(|| ScatterError::Other("server not started".into()))?;
    r.leader
        .validate()
        .await
        .map_err(|e| ScatterError::Other(e.to_string()))?;
    let cluster = coordinator.get_cluster().clone();
    let config = r.state.lock().unwrap().config.clone();
    let single = if request.ids.is_empty() {
        Some(
            cluster
                .regions
                .get_region(request.single)
                .or(request.fallback)
                .ok_or(ScatterError::Missing)?,
        )
    } else {
        None
    };
    // Reserve a bounded block from the fenced allocator for synchronous builder callbacks.
    // One attempt can replace every peer; retries never exceed the canonical limit.
    let regions: Vec<_> = if let Some(region) = &single {
        vec![region.clone()]
    } else {
        request
            .ids
            .iter()
            .filter_map(|id| cluster.regions.get_region(*id))
            .collect()
    };
    let width = regions
        .iter()
        .map(|region| region.meta().peers.len())
        .sum::<usize>()
        .saturating_mul((request.retries.clamp(0, 30) + 1) as usize);
    let mut allocated = std::collections::VecDeque::new();
    if width > 0 {
        let count = u32::try_from(width)
            .map_err(|_| ScatterError::Other("scatter allocation is too large".into()))?;
        let (last, count) = r
            .allocator
            .alloc_count(count)
            .await
            .map_err(|e| ScatterError::Other(e.to_string()))?;
        allocated.extend(last - u64::from(count) + 1..=last);
    }
    let stats = r
        .hot
        .snapshot(0, regions.iter().map(|region| region.meta().id).collect())
        .await
        .map_err(|e| ScatterError::Other(e.to_string()))?;
    let hot = regions
        .iter()
        .filter(|region| stats.is_region_hot(region, config.get_hot_region_cache_hits_threshold()))
        .map(|region| region.meta().id)
        .collect();
    let affinity = astersql_pd_schedule_filter::AffinityFilter {
        manager: None,
        labels,
        now: now().0 as u64,
    };
    let admission = coordinator.operator_handle();
    let mut state = r.state.lock().unwrap();
    let mut scatterer = coordinator.get_region_scatterer(
        &mut state.rules,
        &hot,
        Some(&affinity),
        now().1 as i128,
        true,
    );
    let mut allocate = || {
        allocated
            .pop_front()
            .ok_or_else(|| BuilderError::Allocation("scatter reservation exhausted".into()))
    };
    if let Some(region) = single {
        let mut suspect = Default::default();
        let op = scatterer
            .batch
            .scatter_one(
                &region,
                &request.group,
                request.skip_store_limit,
                &mut suspect,
                &mut allocate,
            )
            .await;
        drop(scatterer);
        coordinator
            .get_checker_controller()
            .lock()
            .unwrap()
            .add_pending_processed_regions(false, &suspect.iter().copied().collect::<Vec<_>>());
        let op = op.map_err(|error| {
            ScatterError::Other(match error {
                ScatterBatchFailure::NotReplicated => {
                    format!("region {} is not fully replicated", region.meta().id)
                }
                ScatterBatchFailure::NoLeader => {
                    format!("region {} has no leader", region.meta().id)
                }
                ScatterBatchFailure::Hot => {
                    format!("region {} is hot: region is hot", region.meta().id)
                }
                ScatterBatchFailure::ExistingOperator => {
                    format!("the operator of region {} already exist", region.meta().id)
                }
                error => error.to_string(),
            })
        })?;
        if let Some(op) = op {
            admission
                .add_operators(&cluster, &[Arc::new(op)])
                .map_err(|_| ScatterError::Rejected)?;
        }
        return Ok(ScatterResult {
            percentage: 100,
            failed: vec![],
        });
    }
    let outcome = scatterer
        .scatter(
            ScatterBatchRequest::Ids(request.ids),
            &request.group,
            request.retries,
            request.skip_store_limit,
            cancel,
            allocate,
        )
        .await
        .map_err(|error| {
            ScatterError::Other(match error {
                ScatterBatchError::EmptyRegion => "[PD:scatter:ErrEmptyRegion]empty region".into(),
                ScatterBatchError::RegionNotFound(_) => {
                    "[PD:region:ErrRegionNotFound]region %v not found".into()
                }
            })
        })?;
    let failures = outcome.failures.len();
    Ok(ScatterResult {
        percentage: if failures == 0 {
            100
        } else {
            (100 - 100 * failures / (outcome.processed + failures)) as u64
        },
        failed: outcome.failures.into_keys().collect(),
    })
}

/// Reuse the worker's retry and influence queues for RPC-created operators.
pub(super) async fn dispatch_new(
    coordinator: &Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    routing: &astersql_pd_schedule_hbstream::routing::RoutingHandle,
    push_queue: &mut astersql_pd_schedule_operator::ConcurrentHeapOpQueue,
    fast: &mut astersql_pd_schedule_operator::FastOperatorInfluence,
    cancel: &CancellationToken,
) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let mut guard = resources.lock().await;
    let Some(r) = guard.as_mut() else {
        return Ok(());
    };
    if r.schedule_creations.is_empty() {
        return Ok(());
    }
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let admission = coordinator.operator_handle();
    let cluster = coordinator.get_cluster();
    let config = r.state.lock().unwrap().config.clone();
    let mut dispatch = astersql_pd_schedule_operator::ControllerDispatch::new(
        &admission,
        cluster,
        &config,
        routing,
        r.paths.cluster_id(),
    );
    dispatch.push_queue = std::mem::take(push_queue);
    dispatch.fast_operators = std::mem::take(fast);
    for id in std::mem::take(&mut r.schedule_creations) {
        if let Some(op) = admission.get_operator(id) {
            dispatch.schedule(op, tokio::time::Instant::now().into_std());
            if let Some(region) = cluster.regions.get_region(id) {
                let out = dispatch
                    .dispatch(
                        &region,
                        astersql_pd_schedule_operator::DispatchSource::Create,
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
    *push_queue = dispatch.push_queue;
    *fast = dispatch.fast_operators;
    Ok(())
}
