// Copyright 2026 AsterSQL.
//! Exclusive management of the same coordinator consumed by schedule_worker.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_schedulers::{
    BalanceRangeJob, BuiltinScheduler, ControlError, JobTime, SchedulerRegistry,
};
use astersql_pd_server::http_scheduler::Command;
use serde_json::{Value, json};
const NOT_FOUND: &str = "[PD:scheduler:ErrSchedulerNotFound]scheduler not found";
fn error(e: ControlError) -> String {
    match e {
        ControlError::NotFound(_) => NOT_FOUND.into(),
        ControlError::Exists(_) => "[PD:scheduler:ErrSchedulerExisted]scheduler existed".into(),
        ControlError::Prepare(errors) => errors
            .first()
            .map(|e| match e {
                astersql_pd_core::StoreControlError::StoreNotFound(id) => {
                    format!("[PD:core:ErrStoreNotFound]store {id} not found")
                }
                e => e.to_string(),
            })
            .unwrap_or_else(|| "scheduler preparation failed".into()),
        e => e.to_string(),
    }
}
pub(super) async fn manage(
    command: Command,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    _cancel: &CancellationToken,
) -> Result<(u16, Value), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let cluster = coordinator.get_cluster().clone();
    let controller = coordinator.get_schedulers_controller_mut();
    match command {
        Command::List { status, timestamps } => {
            let mut values = vec![];
            for name in controller.names() {
                if status == "paused" {
                    if controller.is_paused(&name, now().0).map_err(error)? {
                        if timestamps {
                            let (a, b) = controller.delay(&name, now().0).map_err(error)?;
                            let time = |t| {
                                chrono::DateTime::from_timestamp(t, 0)
                                    .filter(|date| {
                                        (0..=9999).contains(&chrono::Datelike::year(date))
                                    })
                                    .map(|date| {
                                        date.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
                                    })
                                    .ok_or("Time.MarshalJSON: year outside of range [0,9999]")
                            };
                            values.push(
                                json!({"name":name,"paused_at":time(a)?,"resume_at":time(b)?}),
                            )
                        } else {
                            values.push(json!(name))
                        }
                    }
                } else if controller.is_disabled(&name).map_err(error)? == (status == "disabled") {
                    values.push(json!(name))
                }
            }
            Ok((200, json!(values)))
        }
        Command::Pause { name, delay } => match controller.pause_or_resume(&name, delay, now().0) {
            Ok(()) => Ok((200, json!("Pause or resume the scheduler successfully."))),
            Err(e) => Ok((500, json!(error(e)))),
        },
        Command::Create {
            name,
            mut args,
            input,
        } => {
            // The special repeated-create APIs update the existing canonical policy.
            let exists = controller.exists(&name).unwrap_or(false);
            let ctx = controller.context();
            let storage = ctx.storage.clone();
            let paths = ctx.paths;
            if exists
                && matches!(
                    name.as_str(),
                    "evict-leader-scheduler" | "grant-leader-scheduler" | "balance-range-scheduler"
                )
            {
                let update = if name == "evict-leader-scheduler" {
                    json!({"store_ids":args[0].split(',').map(|id|id.parse::<u64>().unwrap()).collect::<Vec<_>>()})
                } else {
                    input.clone()
                };
                let policy = &mut controller.get_mut(&name).unwrap().scheduler.policy;
                let result = match policy {
                    BuiltinScheduler::EvictLeader(p) => p
                        .update_evict_config(&update, &cluster, &storage, &paths)
                        .await
                        .map(|_| json!("The scheduler has been applied to the store."))
                        .map_err(|e| e.to_string()),
                    BuiltinScheduler::GrantLeader(p) => p
                        .update_grant_config(
                            args[0].parse().unwrap(),
                            None,
                            &cluster,
                            &storage,
                            &paths,
                        )
                        .await
                        .map(|_| json!("The scheduler has been applied to the store."))
                        .map_err(|e| e.to_string()),
                    BuiltinScheduler::BalanceRange { jobs, .. } => {
                        match BalanceRangeJob::from_http(
                            input.as_object().unwrap(),
                            JobTime(now().1 as i128),
                        ) {
                            Ok(job) => jobs
                                .add(job)
                                .await
                                .map(|_| Value::Null)
                                .map_err(|e| e.to_string()),
                            Err(e) => Err(e.to_string()),
                        }
                    }
                    _ => unreachable!(),
                };
                return Ok(match result {
                    Ok(v) => (200, v),
                    Err(e) => (
                        if name == "balance-range-scheduler" {
                            400
                        } else {
                            500
                        },
                        json!(e),
                    ),
                });
            }
            for (this, other) in [
                ("grant-hot-region-scheduler", "balance-hot-region-scheduler"),
                ("balance-hot-region-scheduler", "grant-hot-region-scheduler"),
            ] {
                if name == this
                    && controller.exists(other).unwrap_or(false)
                    && !controller.is_disabled(other).unwrap_or(false)
                {
                    return Ok((
                        400,
                        json!(format!("{other} is running, please remove it first")),
                    ));
                }
            }
            if name == "grant-hot-region-scheduler" {
                for (key, message) in [
                    ("store-leader-id", "missing leader id"),
                    ("store-id", "missing store id"),
                ] {
                    let Some(v) = input[key].as_str() else {
                        return Ok((400, json!(message)));
                    };
                    args.push(v.to_owned())
                }
            }
            controller.set_configuration(
                serde_json::to_value(r.persisted_config.persistence_snapshot())
                    .map_err(|e| e.to_string())?,
            );
            let mut registry = SchedulerRegistry::default();
            registry.register_builtins().map_err(|e| e.to_string())?;
            let refs: Vec<_> = args.iter().map(String::as_str).collect();
            let ctx = controller.context();
            let factory = astersql_pd_schedule_schedulers::FactoryContext {
                storage: ctx.storage.clone(),
                paths: ctx.paths,
                now: JobTime(now().1 as i128),
                query_supported: ctx.query_supported,
                cpu_supported: ctx.cpu_supported,
                hot_inputs: ctx.hot_inputs.clone(),
            };
            let policy = match registry.create_args(&name.into(), &refs, &factory) {
                Ok(p) => p,
                Err(e) => return Ok((400, json!(e.to_string()))),
            };
            if let Err(e) = controller.add(policy, &refs, false).await {
                return Ok((400, json!(error(e))));
            }
            // Go publishes the new controller/options even when its final persist fails.
            publish(controller, r);
            let result = storage
                .save_config(&paths, controller.configuration())
                .await;
            match result {
                Ok(()) => Ok((200, json!("The scheduler is created."))),
                Err(e) => Ok((400, json!(e.to_string()))),
            }
        }
        Command::Delete(name) => {
            for base in ["evict-leader-scheduler", "grant-leader-scheduler"] {
                if name.starts_with(base) && name != base {
                    let Some(control) = controller.get_mut(base) else {
                        return Ok((404, Value::Null));
                    };
                    let Some(id) = name
                        .rsplit('-')
                        .next()
                        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                        .and_then(|v| v.parse::<u64>().ok())
                    else {
                        return Ok((400, Value::Null));
                    };
                    let (contains, last) = match &control.scheduler.policy {
                        BuiltinScheduler::EvictLeader(p) => (
                            p.config().stores().contains_key(&id),
                            p.config().stores().len() == 1,
                        ),
                        BuiltinScheduler::GrantLeader(p) => (
                            p.config().stores().contains_key(&id),
                            p.config().stores().len() == 1,
                        ),
                        _ => unreachable!(),
                    };
                    if !contains {
                        return Ok((404, Value::Null));
                    }
                    if last {
                        // Removing the last target removes the policy and its prepared controls.
                        let result = controller.remove(base, false).await;
                        publish(controller, r);
                        return Ok((if result.is_ok() { 200 } else { 500 }, Value::Null));
                    }
                    let ctx = controller.context();
                    let storage = ctx.storage.clone();
                    let paths = ctx.paths;
                    let policy = &mut controller.get_mut(base).unwrap().scheduler.policy;
                    let ok = match policy {
                        BuiltinScheduler::EvictLeader(p) => p
                            .delete_evict_store(id, &cluster, &storage, &paths, |_| Ok(()))
                            .await
                            .is_ok(),
                        BuiltinScheduler::GrantLeader(p) => p
                            .delete_grant_store(id, &cluster, &storage, &paths, |_| Ok(()))
                            .await
                            .is_ok(),
                        _ => unreachable!(),
                    };
                    return Ok((if ok { 200 } else { 500 }, Value::Null));
                }
            }

            controller.set_configuration(
                serde_json::to_value(r.persisted_config.persistence_snapshot())
                    .map_err(|e| e.to_string())?,
            );
            let result = controller.remove(&name, false).await;
            publish(controller, r);
            Ok(match result {
                Ok(()) => (200, json!("The scheduler is removed.")),
                Err(e) => {
                    let status = if matches!(e, ControlError::NotFound(_)) {
                        404
                    } else {
                        500
                    };
                    (status, json!(error(e)))
                }
            })
        }
    }
}
// Controller configuration and host configuration must advance together, including
// Go's retained option changes on a failed remove/persist. Ticks read host state.
fn publish(controller: &astersql_pd_schedule_schedulers::SchedulerController, r: &mut Resources) {
    let configs =
        serde_json::from_value(controller.configuration()["schedule"]["schedulers-v2"].clone())
            .expect("controller emits SchedulerConfig");
    r.persisted_config.config.schedule.schedulers = Some(configs);
    r.state.lock().unwrap().config.schedule.schedulers =
        r.persisted_config.config.schedule.schedulers.clone();
}

pub(super) async fn configure(
    mut request: axum::http::Request<axum::body::Body>,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) -> Result<axum::response::Response, String> {
    use astersql_pd_server::{http_contract::json_response, http_scheduler_config::not_found};
    use axum::{body::Body, http::StatusCode};
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let full = request.uri().clone();
    let suffix = full
        .path()
        .strip_prefix("/pd/api/v1/scheduler-config/")
        .unwrap_or_default();
    let (name, path) = suffix
        .split_once('/')
        .map(|(n, p)| (n, format!("/{p}")))
        .unwrap_or((suffix, String::new()));
    let cluster = coordinator.get_cluster().clone();
    let controller = coordinator.get_schedulers_controller_mut();
    if controller.get_mut(name).is_none() {
        return Ok(not_found());
    }
    if path.is_empty() {
        let location = format!(
            "{}/{}",
            full.path(),
            full.query().map(|q| format!("?{q}")).unwrap_or_default()
        );
        let escaped = location
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&#34;")
            .replace('\'', "&#39;");
        return Ok(axum::response::Response::builder()
            .status(307)
            .header("location", location)
            .header("content-type", "text/html; charset=utf-8")
            .body(Body::from(format!(
                "<a href=\"{escaped}\">Temporary Redirect</a>.\n\n"
            )))
            .unwrap());
    }
    let uri = format!(
        "{path}{}",
        full.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    *request.uri_mut() = uri.parse().map_err(|_| "invalid scheduler path")?;
    let ctx = controller.context();
    let storage = r.storage.clone();
    let paths = ctx.paths;
    let inputs = ctx.hot_inputs.clone();
    let control = controller.get_mut(name).unwrap();
    let disabled = control.scheduler.is_disabled();
    let mut remove = false;
    let response = match &mut control.scheduler.policy {
        BuiltinScheduler::BalanceLeader(p) => {
            astersql_pd_server::schedule_admin_balance_leader::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::BalanceWitness(p) => {
            astersql_pd_server::schedule_admin_balance_witness::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::GrantHot(p) => {
            astersql_pd_server::schedule_admin_grant_hot_region::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::ScatterRange(p) => {
            astersql_pd_server::schedule_admin_scatter_range::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::ShuffleHot(p) => {
            astersql_pd_server::schedule_admin_shuffle_hot_region::serve(
                p, &storage, &paths, request,
            )
            .await
        }
        BuiltinScheduler::ShuffleRegion(p) => {
            astersql_pd_server::schedule_admin_shuffle_region::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::SlowTrend(p) => {
            astersql_pd_server::schedule_admin_evict_slow_trend::serve(p, &storage, &paths, request)
                .await
        }
        BuiltinScheduler::BalanceRegion(p) => {
            astersql_pd_server::schedule_admin_balance_region::serve(p, request)
        }
        BuiltinScheduler::Label(p) => astersql_pd_server::schedule_admin_label::serve(p, request),
        BuiltinScheduler::RandomMerge(p) => {
            astersql_pd_server::schedule_admin_random_merge::serve(p, request)
        }
        BuiltinScheduler::ShuffleLeader(p) => {
            astersql_pd_server::schedule_admin_shuffle_leader::serve(p, request)
        }
        BuiltinScheduler::TransferWitness(p) => {
            astersql_pd_server::schedule_admin_transfer_witness_leader::serve(p, request)
        }
        BuiltinScheduler::EvictLeader(p) => {
            astersql_pd_server::schedule_admin_evict_leader::serve(
                p,
                &cluster,
                &storage,
                &paths,
                request,
                |_| {
                    remove = true;
                    Err(astersql_pd_schedule_schedulers::EvictRemoveError::Failed(
                        "deferred host removal".into(),
                    ))
                },
            )
            .await
        }
        BuiltinScheduler::GrantLeader(p) => {
            astersql_pd_server::schedule_admin_grant_leader::serve(
                p,
                &cluster,
                &storage,
                &paths,
                request,
                |_| {
                    remove = true;
                    Err(astersql_pd_schedule_schedulers::GrantRemoveError::Failed(
                        "deferred host removal".into(),
                    ))
                },
            )
            .await
        }
        BuiltinScheduler::EvictSlowStore { disk, network } => {
            astersql_pd_server::schedule_admin_evict_slow_store::serve(
                disk, network, &storage, &paths, disabled, request,
            )
            .await
        }
        BuiltinScheduler::EvictStoppingStore(p) => {
            astersql_pd_server::schedule_admin_evict_stopping_store::serve(
                p, &storage, &paths, disabled, request,
            )
            .await
        }
        BuiltinScheduler::BalanceRange { jobs, .. } => {
            astersql_pd_server::schedule_admin_balance_range::serve(
                jobs,
                JobTime(now().1 as i128),
                request,
            )
            .await
        }
        BuiltinScheduler::Hot { scheduler, config } => {
            let mut local = astersql_pd_schedule_schedulers::HotRegionConfig::new(config.clone())
                .map_err(|e| e.to_string())?;
            let support = scheduler.observed_support();
            let (observed_query, observed_cpu) = support.unwrap_or((false, false));
            local.set_support(observed_query, observed_cpu);
            let response = astersql_pd_server::schedule_admin_hot_region::serve(
                &mut local, &storage, &paths, request,
            )
            .await;
            if local.params() != config {
                *config = local.params().clone();
                *scheduler = Box::new(
                    astersql_pd_schedule_schedulers::HotScheduler::new(
                        config.clone(),
                        observed_query,
                        observed_cpu,
                        inputs,
                    )
                    .map_err(|e| e.to_string())?,
                );
                if support.is_some() {
                    scheduler.observe_support(observed_query, observed_cpu);
                }
            }
            response
        }
    };
    if remove {
        // The local API's synchronous callback restores its original policy while
        // the host performs async controller removal below. No tick can interleave.

        controller.set_configuration(
            serde_json::to_value(r.persisted_config.persistence_snapshot())
                .map_err(|e| e.to_string())?,
        );
        let result = controller.remove(name, false).await;
        publish(controller, r);
        if let Err(e) = result {
            return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, &error(e)));
        }
        return Ok(json_response(
            StatusCode::OK,
            &"The last store has been deleted",
        ));
    }
    Ok(response)
}

pub(super) fn observe_hot_support(
    policy: &mut BuiltinScheduler,
    version: &astersql_pd_server_config::ClusterVersion,
) -> Result<(), String> {
    if let astersql_pd_schedule_schedulers::BuiltinScheduler::Hot {
        scheduler,
        config: params,
    } = policy
    {
        let query = !super::cluster_split::compare_version(
            version,
            &astersql_pd_server_config::ClusterVersion {
                major: 5,
                minor: 2,
                ..Default::default()
            },
        )
        .is_lt();
        let floor = if version.major >= 9 {
            astersql_pd_server_config::ClusterVersion {
                major: 9,
                pre_release: "beta.1".into(),
                ..Default::default()
            }
        } else {
            astersql_pd_server_config::ClusterVersion {
                major: 8,
                minor: 5,
                patch: 7,
                ..Default::default()
            }
        };
        let cpu = !super::cluster_split::compare_version(version, &floor).is_lt();
        if scheduler.observed_support() != Some((query, cpu)) {
            *scheduler = Box::new(
                astersql_pd_schedule_schedulers::HotScheduler::new(
                    params.clone(),
                    query,
                    cpu,
                    std::array::from_fn(|_| Default::default()),
                )
                .map_err(|e| e.to_string())?,
            );
        }
        scheduler.observe_support(query, cpu);
    }
    Ok(())
}
