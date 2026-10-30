// Copyright 2026 AsterSQL.
//! Store limits share lifecycle serialization, persistence and limiter publication.
use super::*;
use astersql_pd_schedule::Coordinator;
use astersql_pd_server::http_store_state::StoreMutation;
use astersql_pd_storelimit::Type;
pub(super) async fn mutate(
    op: StoreMutation,
    r: &mut Resources,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    let StoreMutation::Limit {
        id,
        labels,
        rate,
        types,
    } = op
    else {
        unreachable!()
    };
    for typ in types {
        let typ = match typ.as_str() {
            "add-peer" => Type::ADD_PEER,
            "remove-peer" => Type::REMOVE_PEER,
            "transfer-leader-in" => Type::TRANSFER_LEADER_IN,
            "send-snapshot" => Type::SEND_SNAPSHOT,
            _ => unreachable!(),
        };
        if let Some(id) = id {
            set_one(id, typ, rate, r, coordinator, storage).await?;
        } else if let Some(labels) = &labels {
            let stores = r.state.lock().unwrap().cluster.stores.get_stores();
            for store in stores.into_iter().filter(|s| !s.is_removed()) {
                for label in labels {
                    if label.key == "engine" && label.value == "tikv" && store.is_tikv() {
                        set_one(store.id(), typ, rate, r, coordinator, storage).await?;
                        continue;
                    }
                    if store.meta().unwrap().labels.iter().any(|l| l == label) {
                        let _ = set_one(store.id(), typ, rate, r, coordinator, storage).await;
                    }
                }
            }
        } else {
            let mut next = r.persisted_config.clone();
            next.config.schedule = r.state.lock().unwrap().config.schedule.clone();
            let cfg = &mut next.config.schedule;
            set_value(&mut cfg.default_store_limit, typ, rate);
            if let Some(limits) = &mut cfg.store_limit {
                for limit in limits.values_mut() {
                    set_value(limit, typ, rate)
                }
            }
            super::store_lifecycle::publish_limits(next, r, coordinator, storage).await?;
            let state = r.state.lock().unwrap();
            use astersql_pd_schedule_config::SharedConfigProvider;
            for store in state.cluster.stores.get_stores() {
                store.reset_store_limit(
                    typ,
                    &[state.config.get_store_limit_by_type(store.id(), typ) / 60.0],
                );
            }
        }
    }
    Ok(())
}
fn set_value(limit: &mut astersql_pd_schedule_config::StoreLimitConfig, typ: Type, rate: f64) {
    match typ {
        Type::ADD_PEER => limit.add_peer = rate,
        Type::REMOVE_PEER => limit.remove_peer = rate,
        Type::TRANSFER_LEADER_IN => limit.transfer_leader_in = rate,
        _ => {}
    }
}
async fn set_one(
    id: u64,
    typ: Type,
    rate: f64,
    r: &mut Resources,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    if r.state
        .lock()
        .unwrap()
        .cluster
        .stores
        .get_store(id)
        .is_some_and(|s| s.is_removed())
    {
        return Err(format!(
            "[PD:core:ErrStoreRemoved]store {id} has been removed"
        ));
    }
    super::store_lifecycle::set_limit(id, typ, rate, r, coordinator, storage).await
}

/// Registration's best-effort limit persistence runs under the same resource gate.
/// A failure does not roll back metadata, matching Go AddStoreLimit's retry policy.
pub(super) async fn ensure_registered_limit(id: u64, r: &mut Resources) {
    for _ in 0..5 {
        let mut next = r.persisted_config.clone();
        next.config.schedule = r.state.lock().unwrap().config.schedule.clone();
        let cfg = &mut next.config.schedule;
        if cfg
            .store_limit
            .as_ref()
            .is_some_and(|limits| limits.contains_key(&id))
        {
            return;
        }
        let store = r.state.lock().unwrap().cluster.stores.get_store(id);
        let Some(store) = store.filter(|s| !s.is_removed()) else {
            return;
        };
        let limit = if astersql_pd_core::is_store_contain_label(store.meta(), "engine", "tiflash") {
            astersql_pd_schedule_config::StoreLimitConfig {
                add_peer: 30.0,
                remove_peer: 30.0,
                transfer_leader_in: astersql_pd_storelimit::UNLIMITED,
            }
        } else {
            cfg.default_store_limit.clone()
        };
        cfg.store_limit
            .get_or_insert_with(Default::default)
            .insert(id, limit);
        if next.persist(&r.storage, &r.paths).await.is_ok() {
            r.state.lock().unwrap().config.schedule = next.config.schedule.clone();
            r.persisted_config = next;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
