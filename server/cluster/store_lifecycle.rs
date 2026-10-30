// Copyright 2026 AsterSQL.
//! Validation and state options shared by manual Store lifecycle admissions.
use astersql_pd_core::{BasicCluster, StoreInfo, StoreState};
use std::collections::BTreeSet;
/// Validate against the canonical snapshot before persisting a state option.
/// None is an idempotent transition, which must not write storage or limits.
pub fn store_transition(
    cluster: &BasicCluster,
    id: u64,
    offline: Option<bool>,
    replicas: i64,
    evicted: &BTreeSet<u64>,
) -> Result<Option<StoreInfo>, String> {
    let store = cluster
        .stores
        .get_store(id)
        .ok_or_else(|| format!("[PD:core:ErrStoreNotFound]store {id} not found"))?;
    if offline.is_some_and(|force| store.is_removing() && store.is_physically_destroyed() == force)
    {
        return Ok(None);
    }
    if store.is_removed() {
        return Err(format!(
            "[PD:core:ErrStoreRemoved]store {id} has been removed"
        ));
    }
    if store.is_physically_destroyed() {
        return Err(format!(
            "[PD:core:ErrStoreDestroyed]store {id} has been physically destroyed"
        ));
    }
    if offline == Some(false) && (store.is_preparing() || store.is_serving()) && store.is_tikv() {
        let up = cluster
            .stores
            .get_stores()
            .into_iter()
            .filter(|s| s.is_up() && s.is_tikv() && s.id() != id)
            .map(|s| s.id())
            .collect::<Vec<_>>();
        if (up.len() as i64) < replicas {
            return Err(format!(
                "[PD:core:ErrStoresNotEnough]can not remove store {id} since the number of up stores would be {} while need {replicas}",
                up.len()
            ));
        }
        if up.iter().all(|id| evicted.contains(id)) {
            return Err(format!(
                "[PD:core:ErrNoStoreForRegionLeader]can not remove store {id} since there are no extra up store to store the leader"
            ));
        }
    }
    if offline.is_none() && store.is_up() {
        return Ok(None);
    }
    let mut next = store.as_ref().clone();
    next.set_store_state(
        if offline.is_some() {
            StoreState::Offline
        } else {
            StoreState::Up
        },
        offline,
    );
    Ok(Some(next))
}
