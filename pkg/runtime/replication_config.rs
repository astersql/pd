// Copyright 2026 AsterSQL.
//! Serialized replication updates, committed with the default placement rule.
use super::*;
use astersql_pd_schedule::Coordinator;

pub(super) async fn update(
    labels: bool,
    patch: serde_json::Value,
    coordinator: &mut Coordinator<ProviderSnapshot<()>>,
    resources: &Arc<AsyncMutex<Option<Resources>>>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let mut guard = resources.lock().await;
    let r = guard.as_mut().ok_or("server not started")?;
    let mut next = r.persisted_config.clone();
    let mut config = r.state.lock().unwrap().config.clone();
    let mut rule_update = None;
    if labels {
        let field = |key: &str| {
            patch
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let action = field("action");
        let typ = field("type");
        let label = astersql_pd_server_config::StoreLabel {
            key: field("label-key"),
            value: field("label-value"),
        };
        let properties = next.config.label_property.get_or_insert_default();
        match action.as_str() {
            "set" => {
                let values = properties.entry(typ).or_default().get_or_insert_default();
                if !values.contains(&label) {
                    values.push(label);
                }
            }
            "delete" => {
                if let Some(values) = properties.get_mut(&typ) {
                    let values = values.get_or_insert_default();
                    values.retain(|v| v != &label);
                    if values.is_empty() {
                        properties.remove(&typ);
                    }
                }
            }
            _ => return Err(format!("unknown action {action}")),
        }
        config.label_properties = properties
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_deref()
                        .unwrap_or_default()
                        .iter()
                        .map(|l| astersql_pd_core::StoreLabel {
                            key: l.key.clone(),
                            value: l.value.clone(),
                        })
                        .collect(),
                )
            })
            .collect();
    } else {
        let old = config.replication.clone();
        let mut value = serde_json::to_value(&old).map_err(|e| e.to_string())?;
        if let Some(fields) = patch.as_object() {
            for (k, v) in fields {
                if !v.is_null() || k == "location-labels" {
                    value[k] = v.clone();
                }
            }
        }
        let candidate: astersql_pd_schedule_config::ReplicationConfig =
            serde_json::from_value(value).map_err(|e| e.to_string())?;
        candidate.validate()?;
        if (candidate.enable_placement_rules
            || candidate.enable_placement_rules != old.enable_placement_rules)
            && r.storage
                .load_meta(&r.paths)
                .await
                .map_err(|e| e.to_string())?
                .is_none()
        {
            return Err("[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first".into());
        }
        let state = r.state.lock().unwrap();
        let stores = state
            .cluster
            .stores
            .get_stores()
            .into_iter()
            .map(|s| s.as_ref().clone())
            .collect::<Vec<_>>();
        if old.enable_placement_rules
            && !candidate.enable_placement_rules
            && stores.iter().any(|s| !s.is_removed() && !s.is_tikv())
        {
            return Err("cannot disable placement rules with TiFlash nodes".into());
        }
        if candidate.enable_placement_rules
            && (candidate.max_replicas != old.max_replicas
                || candidate.location_labels != old.location_labels
                || candidate.isolation_level != old.isolation_level)
        {
            let mut rule = state.rules.get_rule("pd", "default").filter(|r|r.start_key.is_empty() && r.end_key.is_empty()).ok_or("cannot update MaxReplicas, LocationLabels or IsolationLevel when placement rules feature is enabled and not only default rule exists, please update rule instead")?;
            if rule.count != old.max_replicas as i64
                || rule.location_labels != old.location_labels.as_slice()
                || rule.isolation_level != old.isolation_level
            {
                return Err("cannot to update replication config, the default rules do not consistent with replication config, please update rule instead".into());
            }
            rule.count = candidate.max_replicas as i64;
            rule.location_labels = candidate.location_labels.as_slice().to_vec();
            rule.isolation_level = candidate.isolation_level.clone();
            rule_update = Some(
                state
                    .rules
                    .prepare_rule(rule, now().0 as u64, &config, &stores)
                    .map_err(|e| e.to_string())?,
            );
        }
        config.replication = candidate.clone();
        next.config.replication = candidate;
    }
    let document = serde_json::to_vec(&next.persistence_snapshot()).map_err(|e| e.to_string())?;
    let storage = StorageEndpoint::new(Arc::new(crate::member_role_worker::TermStore::new(
        r.meta_store.clone(),
        r.leader.clone(),
        cancel.clone(),
    )));
    storage
        .run_in_txn(async |txn| {
            txn.save(r.paths.config_path().as_bytes(), &document)
                .map_err(astersql_pd_storage_endpoint::EndpointError::from)?;
            if let Some(update) = &rule_update {
                storage.save_rule(txn, &r.paths, &update.rule().store_key(), update.rule())?;
            }
            Ok::<_, astersql_pd_storage_endpoint::EndpointError>(())
        })
        .await
        .map_err(|e| e.to_string())?;
    r.persisted_config = next;
    let mut state = r.state.lock().unwrap();
    if let Some(update) = rule_update {
        state.rules.publish_rule(update);
    }
    state.config = config;
    coordinator.update_config(Arc::new(state.config.clone()));
    coordinator
        .get_schedulers_controller_mut()
        .set_configuration(serde_json::from_slice(&document).expect("serialized configuration"));
    Ok(())
}
