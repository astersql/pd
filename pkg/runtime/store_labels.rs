// Copyright 2026 AsterSQL.
//! Label and weight writes under the coordinator's resource gate and leader fence.
use super::*;
use astersql_pd_server::http_store_state::StoreMutation;

pub(super) async fn mutate(
    op: StoreMutation,
    r: &mut Resources,
    storage: &StorageEndpoint,
) -> Result<(), String> {
    let id = match &op {
        StoreMutation::Labels { id, .. }
        | StoreMutation::DeleteLabel { id, .. }
        | StoreMutation::Weight { id, .. } => *id,
        _ => unreachable!(),
    };
    let weight = matches!(op, StoreMutation::Weight { .. });
    let mut next = r
        .state
        .lock()
        .unwrap()
        .cluster
        .stores
        .get_store(id)
        .ok_or_else(|| {
            if weight {
                format!("[PD:core:ErrStoreNotFound]store {id} not found")
            } else {
                format!("[PD:cluster:ErrInvalidStoreID]invalid store id {id}, not found")
            }
        })?
        .as_ref()
        .clone();
    if next.is_removed() {
        return Err(format!(
            "[PD:core:ErrStoreRemoved]store {id} has been removed"
        ));
    }
    // putStoreImpl checks the supplied metadata before merging labels. Preserve
    // that metadata in address-conflict errors as well as the admission order.
    let mut admission = next.meta().unwrap().clone();
    match &op {
        StoreMutation::Labels { labels, .. } => admission.labels = labels.clone(),
        StoreMutation::DeleteLabel { key, .. } => admission.labels.retain(|l| l.key != *key),
        _ => {}
    }
    match op {
        StoreMutation::Labels { labels, force, .. } => {
            let labels = if force {
                labels
            } else {
                astersql_pd_core::merge_labels(&next.meta().unwrap().labels, &labels)
            };
            next.set_labels(labels);
        }
        StoreMutation::DeleteLabel { key, .. } => {
            let mut labels = next.meta().unwrap().labels.clone();
            let before = labels.len();
            labels.retain(|l| l.key != key);
            if labels.len() == before {
                return Err(format!("the label key {key} does not exist"));
            }
            next.set_labels(labels);
        }
        StoreMutation::Weight { leader, region, .. } => {
            storage
                .save_store_weight(&r.paths, id, leader, region)
                .await
                .map_err(|e| e.to_string())?;
            next.set_leader_weight(leader);
            next.set_region_weight(region);
        }
        _ => unreachable!(),
    }
    if !weight {
        let state = r.state.lock().unwrap();
        check_store_admission(
            &admission,
            &r.persisted_config.config.cluster_version,
            &state.cluster,
        )
        .map_err(|e| e.to_string())?;
        let config = &state.config.replication;
        astersql_pd_cluster::check_store_location_labels(
            &next,
            config.location_labels.as_slice(),
            config.strictly_match_label,
        )?;
    }

    storage
        .save_store_meta(&r.paths, next.meta().unwrap())
        .await
        .map_err(|e| e.to_string())?;
    r.state.lock().unwrap().cluster.stores.patch_store(id, |s| {
        if weight {
            s.set_leader_weight(next.leader_weight());
            s.set_region_weight(next.region_weight());
        } else {
            s.set_labels(next.meta().unwrap().labels.clone());
        }
    });
    Ok(())
}
