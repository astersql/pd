// Copyright 2026 AsterSQL.
//! Lifecycle commands share coordinator serialization and the metadata admission gate.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_schedulers::BuiltinScheduler;
use astersql_pd_server::http_store_state::StoreMutation;
use astersql_pd_storelimit::{Type, UNLIMITED};
impl ClusterHost {
    pub(crate) async fn mutate_store(&self, operation: StoreMutation) -> Result<(), String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.schedule_requests
            .send(schedule_scatter::Command::Store(operation, tx))
            .await
            .map_err(|_| "scheduling stopped")?;
        rx.await.map_err(|_| "scheduling stopped")?
    }
}

pub(super) async fn mutate(
    operation: StoreMutation,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    r.leader.validate().await.map_err(|e| e.to_string())?;
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    if matches!(operation, StoreMutation::RemoveTombstones) {
        let stores = r.state.lock().unwrap().cluster.stores.get_stores();
        let mut failed = Vec::new();
        for store in stores.into_iter().filter(|s| s.is_removed()) {
            if r.state
                .lock()
                .unwrap()
                .cluster
                .regions
                .store_region_count(store.id())
                > 0
            {
                failed.push(store.id().to_string());
                continue;
            }
            remove_limits(store.id(), r, coordinator, &storage).await?;
            storage
                .delete_store_meta(&r.paths, store.meta().unwrap())
                .await
                .map_err(|e| e.to_string())?;
            let mut state = r.state.lock().unwrap();
            state.cluster.stores.delete_store(store.id());
            r.previous_store_limits.remove(&store.id());
            state.rules.remove_store_cache(store.id());
            r.hot.stores_stats.remove_rolling_store_stats(store.id());
        }
        return if failed.is_empty() {
            Ok(())
        } else {
            Err(format!("failed stores: {}", failed.join(", ")))
        };
    }
    let (id, offline) = match operation {
        StoreMutation::Offline { id, force } => (id, Some(force)),
        StoreMutation::Up(id) => (id, None),
        _ => unreachable!(),
    };
    let evicted = coordinator
        .get_schedulers_controller()
        .handler("evict-leader-scheduler")
        .and_then(|h| match &h.policy {
            BuiltinScheduler::EvictLeader(p) => Some(p.config().stores().keys().copied().collect()),
            _ => None,
        })
        .unwrap_or_default();
    let next = {
        let state = r.state.lock().unwrap();
        astersql_pd_cluster::store_transition(
            &state.cluster,
            id,
            offline,
            state.config.replication.max_replicas as i64,
            &evicted,
        )?
    };
    let Some(next) = next else {
        return Ok(());
    };
    storage
        .save_store_meta(&r.paths, next.meta().unwrap())
        .await
        .map_err(|e| e.to_string())?;
    {
        let state = r.state.lock().unwrap();
        state
            .cluster
            .stores
            .patch_store(id, |s| s.set_store_state(next.state(), offline));
    }
    let current = {
        let state = r.state.lock().unwrap();
        let schedule = &state.config.schedule;
        schedule
            .store_limit
            .as_ref()
            .and_then(|m| m.get(&id))
            .unwrap_or(&schedule.default_store_limit)
            .clone()
    };
    if offline.is_some() {
        r.previous_store_limits.insert(id, current);
        // Go reports the committed state even if the subsequent limit write fails.
        let _ = set_limit(id, Type::REMOVE_PEER, UNLIMITED, r, coordinator, &storage).await;
    } else if let Some(previous) = r.previous_store_limits.get(&id).cloned() {
        next.reset_store_limit(Type::ADD_PEER, &[previous.add_peer]);
        next.reset_store_limit(Type::REMOVE_PEER, &[previous.remove_peer]);
        // Keep the two best-effort commits separate: the first failure must not
        // suppress the second write or turn the already committed Up into an error.
        let _ = set_limit(
            id,
            Type::ADD_PEER,
            previous.add_peer,
            r,
            coordinator,
            &storage,
        )
        .await;
        let _ = set_limit(
            id,
            Type::REMOVE_PEER,
            previous.remove_peer,
            r,
            coordinator,
            &storage,
        )
        .await;
    }
    Ok(())
}
async fn set_limit(
    id: u64,
    typ: Type,
    rate: f64,
    r: &mut Resources,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    let mut config = r.persisted_config.clone();
    config.config.schedule = r.state.lock().unwrap().config.schedule.clone();
    let schedule = &mut config.config.schedule;
    let limit = schedule
        .store_limit
        .get_or_insert_with(Default::default)
        .entry(id)
        .or_insert_with(|| schedule.default_store_limit.clone());
    match typ {
        Type::ADD_PEER => limit.add_peer = rate,
        Type::REMOVE_PEER => limit.remove_peer = rate,
        _ => unreachable!(),
    }
    publish_limits(config, r, coordinator, storage).await?;
    if let Some(store) = r.state.lock().unwrap().cluster.stores.get_store(id) {
        store.reset_store_limit(typ, &[rate / 60.0]);
    }
    Ok(())
}

async fn publish_limits(
    next: astersql_pd_server_config::PersistedConfig,
    r: &mut Resources,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    next.persist(storage, &r.paths)
        .await
        .map_err(|e| e.to_string())?;
    let document = serde_json::to_value(next.persistence_snapshot()).map_err(|e| e.to_string())?;
    r.persisted_config = next;
    let mut state = r.state.lock().unwrap();
    state.config.schedule = r.persisted_config.config.schedule.clone();
    coordinator.update_config(Arc::new(state.config.clone()));
    coordinator
        .get_schedulers_controller_mut()
        .set_configuration(document);
    Ok(())
}

/// Retry the config deletion before deleting metadata, as Go RemoveStoreLimit does.
async fn remove_limits(
    id: u64,
    r: &mut Resources,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    let mut error = String::new();
    for _ in 0..5 {
        let mut next = r.persisted_config.clone();
        next.config.schedule = r.state.lock().unwrap().config.schedule.clone();
        let removed = next
            .config
            .schedule
            .store_limit
            .as_mut()
            .and_then(|m| m.remove(&id))
            .is_some();
        let result = if removed {
            publish_limits(next, r, coordinator, storage).await
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {
                if let Some(store) = r.state.lock().unwrap().cluster.stores.get_store(id) {
                    for typ in [Type::ADD_PEER, Type::REMOVE_PEER, Type::TRANSFER_LEADER_IN] {
                        store.reset_store_limit(typ, &[]);
                    }
                }
                return Ok(());
            }
            Err(e) => error = e,
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(error)
}
pub(super) async fn cleanup(
    ids: Vec<u64>,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) {
    let mut guard = resources.lock().await;
    let Some(r) = guard.as_mut() else {
        return;
    };
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    for id in ids {
        if r.state
            .lock()
            .unwrap()
            .cluster
            .stores
            .get_store(id)
            .is_some_and(|s| !s.is_removed())
        {
            continue;
        }
        r.previous_store_limits.remove(&id);
        // A failed limit removal must not undo an already persisted tombstone.
        let _ = remove_limits(id, r, coordinator, &storage).await;
    }
}
