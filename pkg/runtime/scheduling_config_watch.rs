// Copyright 2026 AsterSQL.
//! PD-owned options observed by the Scheduling term, published by its actor.
use super::*;
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_schedule::Coordinator;
use astersql_pd_server_config::ConfigDefaultsContext;
use astersql_pd_storage::Event;
use std::time::Duration;
pub(crate) async fn run(
    cluster: Arc<ClusterHost>,
    mut watch: ReconciledWatch,
    cancel: CancellationToken,
) {
    let mut dirty = true;
    while !cancel.is_cancelled() {
        if dirty {
            match cluster.reload_scheduling_configuration().await {
                Ok(()) => dirty = false,
                Err(error) => {
                    if !cancel.is_cancelled() {
                        tracing::warn!(component="scheduling",%error,"configuration reload failed");
                    }
                    tokio::select! {biased;_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
                    continue;
                }
            }
        }
        // Go ignores config deletion. A replacement after compaction is decoded
        // through the same canonical reload as a live Put, with no write-back.
        let result = watch
            .apply_next(&cancel, |batch| {
                dirty |= batch.events.iter().any(|e| matches!(e, Event::Put(_)));
                Ok(())
            })
            .await;
        if result.is_err() {
            tokio::select! {biased;_=cancel.cancelled()=>return,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
        }
    }
}
pub(super) async fn reload(
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
) -> Result<(), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    if r.role != ClusterRole::Scheduling {
        return Err("configuration observation requires Scheduling role".into());
    }
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let mut next = r.persisted_config.clone();
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    if !next
        .reload(
            &r.storage,
            &r.paths,
            ConfigDefaultsContext {
                hostname: "",
                cwd: &cwd,
            },
            &mut Default::default(),
        )
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    let mut provider = r.state.lock().unwrap().config.clone();
    crate::cluster_restore::apply_persisted_config(&next, &mut provider)
        .map_err(|e| e.to_string())?;
    let document = serde_json::to_value(next.persistence_snapshot()).map_err(|e| e.to_string())?;
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
