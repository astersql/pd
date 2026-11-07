// Copyright 2026 AsterSQL.
//! PD-owned options observed by the Scheduling term, published by its actor.
use super::*;
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_schedule::Coordinator;
use astersql_pd_server_config::ConfigDefaultsContext;
use astersql_pd_storage::Event;
use std::{collections::VecDeque, time::Duration};
#[derive(Debug)]
pub(crate) enum ApplyError {
    Invalid(String),
    Transient(String),
}
pub(crate) async fn run(
    cluster: Arc<ClusterHost>,
    mut watch: ReconciledWatch,
    cancel: CancellationToken,
) {
    let mut pending = VecDeque::new();
    while !cancel.is_cancelled() {
        if let Some(document) = pending.front().cloned() {
            match cluster.observe_scheduling_configuration(document).await {
                Ok(()) => {
                    pending.pop_front();
                }
                Err(ApplyError::Invalid(error)) => {
                    // Like Go's putFn, a bad event preserves the last valid
                    // generation and must not poison later valid events.
                    tracing::warn!(component="scheduling",%error,"observed configuration rejected");
                    pending.pop_front();
                }
                Err(ApplyError::Transient(error)) => {
                    if !cancel.is_cancelled() {
                        tracing::warn!(component="scheduling",%error,"configuration application deferred");
                    }
                    tokio::select! {biased;_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
                }
            }
            continue;
        }
        // Keep each event value in order. Re-reading the latest key could skip
        // a valid event when a later malformed write has already arrived.
        let result = watch
            .apply_next(&cancel, |batch| {
                pending.extend(batch.events.iter().filter_map(|e| match e {
                    Event::Put(row) => Some(row.value.clone()),
                    Event::Delete { .. } => None,
                }));
                Ok(())
            })
            .await;
        if result.is_err() {
            tokio::select! {biased;_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
        }
    }
}
pub(super) async fn apply(
    document: Vec<u8>,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) -> Result<(), ApplyError> {
    let mut guard = resources.lock().await;
    let r = guard
        .as_mut()
        .ok_or_else(|| ApplyError::Transient("server not started".into()))?;
    if r.role != ClusterRole::Scheduling {
        return Err(ApplyError::Transient(
            "configuration observation requires Scheduling role".into(),
        ));
    }
    r.leader
        .validate()
        .await
        .map_err(|e| ApplyError::Transient(e.to_string()))?;
    let mut next = r.persisted_config.clone();
    let cwd = std::env::current_dir().map_err(|e| ApplyError::Transient(e.to_string()))?;
    next.apply_document(
        &document,
        ConfigDefaultsContext {
            hostname: "",
            cwd: &cwd,
        },
        &mut Default::default(),
    )
    .map_err(|e| ApplyError::Invalid(e.to_string()))?;
    let mut provider = r.state.lock().unwrap().config.clone();
    crate::cluster_restore::apply_persisted_config(&next, &mut provider)
        .map_err(|e| ApplyError::Invalid(e.to_string()))?;
    let document = serde_json::to_value(next.persistence_snapshot())
        .map_err(|e| ApplyError::Invalid(e.to_string()))?;
    // No await between validation and publication of one shared generation.
    r.persisted_config = next;
    let mut state = r.state.lock().unwrap();
    state.config = provider;
    coordinator.update_config(Arc::new(state.config.clone()));
    coordinator
        .get_schedulers_controller_mut()
        .set_configuration(document);
    if let Some(metrics) = &r.metrics {
        metrics
            .classic
            .scheduling_metrics(state.config.schedule.halt_scheduling);
    }
    Ok(())
}
