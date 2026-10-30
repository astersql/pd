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
    Hotspot(
        astersql_pd_server::http_hotspot::Query,
        oneshot::Sender<Result<serde_json::Value, String>>,
    ),
    Replicated(String, String, oneshot::Sender<Result<String, String>>),
    RegionLabel(
        Arc<astersql_pd_core::RegionInfo>,
        String,
        oneshot::Sender<Result<String, String>>,
    ),
    StoreCleanup(Vec<u64>),
    Store(
        astersql_pd_server::http_store_state::StoreMutation,
        oneshot::Sender<Result<(), String>>,
    ),
    ServerConfig(
        String,
        serde_json::Value,
        oneshot::Sender<Result<(), String>>,
    ),
    Replication(bool, serde_json::Value, oneshot::Sender<Result<(), String>>),
    Configure(serde_json::Value, oneshot::Sender<Result<(), String>>),
    Split(super::schedule_split::Pending),
    Scatter(
        ScatterRequest,
        oneshot::Sender<Result<ScatterResult, ScatterError>>,
    ),
    Operator(u64, oneshot::Sender<Result<Option<OperatorResult>, String>>),
}
impl ClusterHost {
    pub(crate) async fn region_label(
        &self,
        region: Arc<astersql_pd_core::RegionInfo>,
        key: String,
    ) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        tokio::select! {biased;
            _=self.cancel.cancelled()=>Err("server not started".into()),
            result=async {self.schedule_requests.send(Command::RegionLabel(region,key,tx)).await.map_err(|_|"scheduling stopped")?;rx.await.map_err(|_|"scheduling stopped")?}=>result,
        }
    }

    pub(crate) async fn patch_server_config(
        &self,
        key: String,
        value: serde_json::Value,
    ) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.schedule_requests
            .send(Command::ServerConfig(key, value, tx))
            .await
            .map_err(|_| "scheduling stopped")?;
        rx.await.map_err(|_| "scheduling stopped")?
    }

    pub(crate) async fn patch_replication(
        &self,
        labels: bool,
        patch: serde_json::Value,
    ) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.schedule_requests
            .send(Command::Replication(labels, patch, tx))
            .await
            .map_err(|_| "scheduling stopped")?;
        rx.await.map_err(|_| "scheduling stopped")?
    }

    pub(crate) async fn patch_schedule(&self, patch: serde_json::Value) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.schedule_requests
            .send(Command::Configure(patch, tx))
            .await
            .map_err(|_| "scheduling stopped")?;
        rx.await.map_err(|_| "scheduling stopped")?
    }

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
    splits: &mut Vec<super::schedule_split::Pending>,
) {
    match command {
        Command::Hotspot(query, mut reply) => {
            let work = async {
                use astersql_pd_server::http_hotspot::Query;
                use astersql_pd_statistics_utils::RWType;
                let guard = resources.lock().await;
                let r = guard.as_ref().ok_or("server not started")?;
                r.leader.validate().await.map_err(|e| e.to_string())?;
                let handler = astersql_pd_schedule_handler::Handler::new(Some(coordinator));
                let value = match query {
                    Query::Store(id) => {
                        handler.get_store(id).map_err(|_| {
                            format!("[PD:core:ErrStoreNotFound]store {} not found", id)
                        })?;
                        return Ok(serde_json::Value::Null);
                    }
                    Query::Buckets(ids) => serde_json::to_value(
                        handler
                            .get_hot_buckets(&r.hot.bucket_client(), &ids, cancel)
                            .await
                            .map_err(|e| e.to_string())?,
                    ),
                    Query::Stores => serde_json::to_value(
                        handler.get_hot_stores(&r.hot).map_err(|e| e.to_string())?,
                    ),
                    Query::Regions { write, ids } => {
                        use astersql_pd_statistics::StoreStatInformer;
                        let rw = if write { RWType::WRITE } else { RWType::READ };
                        let mut degree = coordinator
                            .get_config()
                            .get_hot_region_cache_hits_threshold();
                        // Go's scheduling controller scales read degrees to the
                        // 60s region reporting window (store reports arrive every 10s).
                        if rw == RWType::READ {
                            degree = degree.wrapping_mul(
                                (RWType::WRITE.report_interval() / RWType::READ.report_interval())
                                    as i64,
                            );
                        }
                        let peers = r
                            .hot
                            .hot_cache
                            .get_hot_peer_stats(rw, degree)
                            .await
                            .map_err(|e| e.to_string())?
                            .into_iter()
                            .collect();
                        let empty = std::collections::HashMap::new();
                        let loads = r.hot.get_stores_loads();
                        serde_json::to_value(
                            handler
                                .get_hot_regions(
                                    if write { RWType::WRITE } else { RWType::READ },
                                    &ids,
                                    &astersql_pd_schedule::HotObservationInputs {
                                        loads: &loads,
                                        read: if write { &empty } else { &peers },
                                        write: if write { &peers } else { &empty },
                                    },
                                )
                                .map_err(|e| e.to_string())?,
                        )
                    }
                };
                value.map_err(|e| e.to_string())
            };
            tokio::select! {biased;_=cancel.cancelled()=>{},_=reply.closed()=>{},result=work=>{let _=reply.send(result);}}
        }

        Command::Replicated(start, end, mut reply) => {
            let work = async {
                let guard = resources.lock().await;
                let r = guard.as_ref().ok_or("server not started")?;
                r.leader.validate().await.map_err(|e| e.to_string())?;
                let mut state = r.state.lock().unwrap();
                astersql_pd_schedule_handler::Handler::new(Some(coordinator))
                    .check_regions_replicated(&start, &end, &mut state.rules, now().1 as i128)
                    .map(str::to_owned)
                    .map_err(|e| e.to_string())
            };
            tokio::select! {biased;_=cancel.cancelled()=>{},_=reply.closed()=>{},r=work=>{let _=reply.send(r);}}
        }

        Command::RegionLabel(region, key, reply) => {
            let _ = reply.send(Ok(labels.get_region_label(&region, &key, now().1 as i128)));
        }

        Command::StoreCleanup(ids) => {
            tokio::select! {biased;_=cancel.cancelled()=>{},_=super::store_lifecycle::cleanup(ids,coordinator,resources,cancel)=>{}}
        }

        Command::Store(operation, mut reply) => {
            let work = super::store_lifecycle::mutate(operation, coordinator, resources, cancel);
            tokio::select! {biased;_=cancel.cancelled()=>{},_=reply.closed()=>{},r=work=>{let _=reply.send(r);}}
        }

        Command::ServerConfig(key, value, mut reply) => {
            let work = super::server_config::update(key, value, coordinator, resources, cancel);
            tokio::select! {biased; _=cancel.cancelled()=>{},_=reply.closed()=>{},result=work=>{let _=reply.send(result);}}
        }

        Command::Replication(labels, patch, mut reply) => {
            let work =
                super::replication_config::update(labels, patch, coordinator, resources, cancel);
            tokio::select! { biased; _ = cancel.cancelled() => {}, _ = reply.closed() => {}, result = work => {let _ = reply.send(result);} }
        }

        Command::Configure(patch, mut reply) => {
            let work = async {
                let mut guard = resources.lock().await;
                let r = guard.as_mut().ok_or("server not started")?;
                let mut next = r.persisted_config.clone();
                next.config.schedule = r.state.lock().unwrap().config.schedule.clone();
                let mut value =
                    serde_json::to_value(&next.config.schedule).map_err(|e| e.to_string())?;
                let mut scalars = patch.clone();
                if let Some(fields) = scalars.as_object_mut() {
                    fields.remove("store-limit");
                }
                overlay_schedule(&mut value, &scalars);
                if let Some(limits) = patch
                    .get("store-limit")
                    .and_then(serde_json::Value::as_object)
                {
                    let defaults = value["default-store-limit"].clone();
                    if value["store-limit"].is_null() {
                        value["store-limit"] = serde_json::json!({});
                    }
                    for (id, patch) in limits {
                        let mut limit = value["store-limit"]
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| defaults.clone());
                        overlay_schedule(&mut limit, patch);
                        value["store-limit"][id] = limit;
                    }
                }
                next.config.schedule =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                next.config.schedule.validate()?;
                for key in [
                    "disable-raft-learner",
                    "disable-remove-down-replica",
                    "disable-replace-offline-replica",
                    "disable-make-up-replica",
                    "disable-remove-extra-replica",
                    "disable-location-replacement",
                    "store-balance-rate",
                ] {
                    if value.get(key).is_some_and(|v| {
                        v == "true"
                            || (key == "store-balance-rate" && v.as_f64().is_some_and(|n| n != 0.0))
                    }) {
                        return Err(format!("{key} has already been deprecated"));
                    }
                }
                let storage =
                    StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
                        r.meta_store.clone(),
                        r.leader.clone(),
                        cancel.clone(),
                    )));
                let scheduler_configuration =
                    serde_json::to_value(next.persistence_snapshot()).map_err(|e| e.to_string())?;
                next.persist(&storage, &r.paths)
                    .await
                    .map_err(|e| e.to_string())?;
                // No await between durable completion and publication to all consumers.
                r.persisted_config = next;
                let mut state = r.state.lock().unwrap();
                state.config.schedule = r.persisted_config.config.schedule.clone();
                coordinator.update_config(Arc::new(state.config.clone()));
                coordinator
                    .get_schedulers_controller_mut()
                    .set_configuration(scheduler_configuration);
                Ok(())
            };
            tokio::select! {biased; _=cancel.cancelled()=>{}, _=reply.closed()=>{}, result=work=>{let _=reply.send(result);}}
        }

        Command::Split(pending) => splits.push(pending),
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

// Go JSON patches retain scalar values on null, recursively merge objects, and
// replace arrays. Unknown fields are ignored by the canonical typed decoder.
fn overlay_schedule(base: &mut serde_json::Value, patch: &serde_json::Value) {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            if let Some(old) = base.get_mut(key) {
                overlay_schedule(old, value);
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    } else if !patch.is_null() || base.is_array() || base.is_null() {
        *base = patch.clone();
    }
}
