// Copyright 2026 AsterSQL.
//! One manager and joined replication worker per serving term.
use super::*;
use astersql_pd_replication::{
    ModeManager, ModeMetrics, ModePhase, ModeSnapshot, ModeWorker, StoreDRStatus,
};

pub(super) async fn manager(
    config: astersql_pd_replication::ReplicationModeConfig,
    store: Arc<dyn MetaStore>,
    paths: ClusterPaths,
    allocator: Arc<astersql_pd_id::Allocator>,
    cancel: CancellationToken,
    metrics: Option<ModeMetrics>,
) -> Result<Arc<ModeManager>, crate::NodeError> {
    let manager = Arc::new(
        ModeManager::new(
            config,
            Arc::new(astersql_pd_storage_domain::StorageEndpoint::new(store)),
            paths,
            allocator,
            cancel,
        )
        .await?,
    );
    if let Some(metrics) = metrics {
        manager.set_metrics(metrics);
    }
    Ok(manager)
}
pub(super) fn start(
    resources: Arc<AsyncMutex<Option<Resources>>>,
    cancel: CancellationToken,
    metrics: Option<ModeMetrics>,
) -> ModeWorker {
    let token = cancel.clone();
    ModeWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        metrics,
        move |phase| {
            let resources = resources.clone();
            let cancel = token.clone();
            async move {
                let result = async {
                    let guard = resources.lock().await;
                    let r = guard.as_ref().ok_or("server not started")?;
                    if cancel.is_cancelled() {
                        return Err("operation cancelled".into());
                    }
                    r.leader.validate().await.map_err(|e| e.to_string())?;
                    let Some(manager) = r.replication.clone() else {
                        return Ok(());
                    };
                    match phase {
                        ModePhase::Update => {
                            let (stores, rules, regions) = {
                                let state = r.state.lock().unwrap();
                                (
                                    state.cluster.stores.get_stores(),
                                    state.rules.get_all_rules(),
                                    state.cluster.regions.clone(),
                                )
                            };
                            manager
                                .tick(&stores, &rules, &regions, now().1 as i128)
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                        ModePhase::Replicate => {
                            let snapshot = manager.snapshot().await;
                            if let Some(metrics) = &r.metrics {
                                metrics.replication.publish(&snapshot);
                            }
                            let (store, paths, config, cache) = (
                                r.meta_store.clone(),
                                r.paths,
                                r.replication_transport_config
                                    .as_deref()
                                    .unwrap_or(&r.persisted_config.config)
                                    .clone(),
                                r.replicated_states.clone(),
                            );
                            drop(guard);
                            let mut cache = cache.lock().await;
                            astersql_pd_server::replication_mode::replicate(
                                store.as_ref(),
                                paths.cluster_id(),
                                &snapshot,
                                &config,
                                &mut cache,
                                cancel,
                            )
                            .await?;
                        }
                    }
                    Ok::<_, String>(())
                }
                .await;
                if let Err(error) = result {
                    let guard = resources.lock().await;
                    if let Some(r) = guard.as_ref() {
                        *r.replication_error.lock().unwrap() = Some(error);
                    }
                }
            }
        },
    )
}
impl ClusterHost {
    pub(crate) async fn set_replication_transport_config(
        &self,
        config: Option<Arc<astersql_pd_server_config::Config>>,
    ) {
        if let Some(r) = self.resources.lock().await.as_mut() {
            r.replication_transport_config = config;
        }
    }

    pub(crate) async fn replication_snapshot(&self) -> Result<Option<ModeSnapshot>, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        if self.cancel.is_cancelled() {
            return Err("server not started".into());
        }
        r.leader.validate().await.map_err(|e| e.to_string())?;
        match &r.replication {
            Some(manager) => Ok(Some(manager.snapshot().await)),
            None => Ok(None),
        }
    }
    pub(crate) async fn report_store_dr_status(
        &self,
        store: u64,
        status: StoreDRStatus,
    ) -> Result<(), String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("server not started")?;
        if self.cancel.is_cancelled() {
            return Err("server not started".into());
        }
        r.leader.validate().await.map_err(|e| e.to_string())?;
        let manager = r
            .replication
            .as_ref()
            .ok_or("replication manager is not configured")?;
        manager.update_store_dr_status(store, status).await;
        Ok(())
    }
}
