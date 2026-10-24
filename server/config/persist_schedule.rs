// Copyright 2026 AsterSQL.
//! Host-owned configuration persistence through the canonical endpoint.
use crate::{Config, ConfigDefaultsContext};
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::{ScheduleDefaults, StoreConfig};
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The original Config JSON envelope with the injected TiKV store configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedConfig {
    #[serde(flatten)]
    pub config: Config,
    pub store: StoreConfig,
}
/// Persistence errors do not change the host's active configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigPersistenceError {
    #[error(transparent)]
    Endpoint(#[from] EndpointError),
    #[error("invalid persisted configuration: {0}")]
    Invalid(String),
}
impl PersistedConfig {
    /// Save the dynamic Config sections and injected store, matching PersistOptions.Persist.
    pub async fn persist(
        &self,
        endpoint: &StorageEndpoint,
        paths: &ClusterPaths,
    ) -> Result<(), ConfigPersistenceError> {
        let mut snapshot = Self::default();
        copy_dynamic(&mut snapshot.config, &self.config);
        snapshot.store = self.store.clone();
        endpoint.save_config(paths, &snapshot).await?;
        Ok(())
    }
    /// Build and validate a candidate before publishing the dynamic sections.
    /// Returns false for absent/empty storage. Host facts and process defaults are explicit.
    pub async fn reload(
        &mut self,
        endpoint: &StorageEndpoint,
        paths: &ClusterPaths,
        context: ConfigDefaultsContext<'_>,
        defaults: &mut ScheduleDefaults,
    ) -> Result<bool, ConfigPersistenceError> {
        let mut next_defaults = defaults.clone();
        let mut candidate = Self::default();
        candidate
            .config
            .adjust(
                &ConfigMetaData::default(),
                context,
                true,
                &mut next_defaults,
            )
            .map_err(ConfigPersistenceError::Invalid)?;
        let Some(raw) = endpoint.load_config::<Value>(paths).await? else {
            return Ok(false);
        };
        let mut value = serde_json::to_value(&candidate).map_err(EndpointError::JsonMarshal)?;
        if !raw.is_object() && !raw.is_null() {
            return Err(ConfigPersistenceError::Invalid(
                "expected configuration object".into(),
            ));
        }
        check_shape(&value, &raw).map_err(ConfigPersistenceError::Invalid)?;
        overlay(&mut value, raw.clone());
        // Validate before removing legacy fields: malformed values must not be
        // silently erased by migration. Go unmarshals typed fields first too.
        let _: PersistedConfig =
            serde_json::from_value(value.clone()).map_err(EndpointError::JsonUnmarshal)?;
        if let Some(stores) = raw
            .get("schedule")
            .and_then(|s| s.get("store-limit"))
            .and_then(Value::as_object)
        {
            for store in stores.values() {
                if !store.is_null() && !store.is_object() {
                    return Err(ConfigPersistenceError::Invalid(
                        "expected store limit object".into(),
                    ));
                }
            }
        }
        migrate_schedule(&mut value, &raw, &next_defaults);
        candidate = serde_json::from_value(value).map_err(EndpointError::JsonUnmarshal)?;
        candidate
            .config
            .pd_server
            .validate_gc_concurrency()
            .map_err(|e| ConfigPersistenceError::Invalid(e.into()))?;
        let schedulers = candidate.config.schedule.schedulers.get_or_insert_default();
        for scheduler in &next_defaults.schedulers {
            if !schedulers.iter().any(|s| s.r#type == scheduler.r#type) {
                schedulers.push(scheduler.clone());
            }
        }
        candidate.store.adjust();
        next_defaults.store_limit = candidate.config.schedule.default_store_limit.clone();
        copy_dynamic(&mut self.config, &candidate.config);
        self.store = candidate.store;
        *defaults = next_defaults;
        Ok(true)
    }
}
fn copy_dynamic(to: &mut Config, from: &Config) {
    to.schedule = from.schedule.clone();
    to.replication = from.replication.clone();
    to.pd_server = from.pd_server.clone();
    to.replication_mode = from.replication_mode.clone();
    to.label_property = from.label_property.clone();
    to.keyspace = from.keyspace.clone();
    to.microservice = from.microservice.clone();
    to.cluster_version = from.cluster_version.clone();
}
fn overlay(base: &mut Value, patch: Value) {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            match base.get_mut(key) {
                Some(old) if !value.is_null() => overlay(old, value.clone()),
                Some(old) if old.is_null() || old.is_array() => *old = Value::Null,
                Some(_) => {}
                None => {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
    } else if !patch.is_null() {
        *base = patch;
    }
}

// Migration consumes persisted field presence only at the adapter boundary.
fn migrate_schedule(value: &mut Value, raw: &Value, defaults: &ScheduleDefaults) {
    let Some(schedule) = value.get_mut("schedule").and_then(Value::as_object_mut) else {
        return;
    };
    let input = raw.get("schedule").unwrap_or(&Value::Null);
    let rate = schedule
        .get("store-balance-rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let limits = schedule
        .get_mut("default-store-limit")
        .unwrap()
        .as_object_mut()
        .unwrap();
    for (key, fallback) in [
        ("add-peer", defaults.store_limit.add_peer),
        ("remove-peer", defaults.store_limit.remove_peer),
        (
            "transfer-leader-in",
            defaults.store_limit.transfer_leader_in,
        ),
    ] {
        if input
            .get("default-store-limit")
            .and_then(|v| v.get(key))
            .is_none_or(Value::is_null)
        {
            limits.insert(
                key.into(),
                serde_json::json!(if rate != 0.0 && key != "transfer-leader-in" {
                    rate
                } else {
                    fallback
                }),
            );
        }
    }
    let leader = limits["transfer-leader-in"].clone();
    // Persisted legacy entries used zero for omitted peer limits; leader is newer.
    if let Some(stores) = input.get("store-limit").and_then(Value::as_object) {
        let mut migrated = serde_json::Map::new();
        for (id, store) in stores {
            let mut limit =
                serde_json::json!({"add-peer":0,"remove-peer":0,"transfer-leader-in":leader});
            overlay(&mut limit, store.clone());
            migrated.insert(id.clone(), limit);
        }
        schedule.insert("store-limit".into(), Value::Object(migrated));
    }
    schedule.remove("store-balance-rate");
    schedule.remove("disable-raft-learner");
    for flag in [
        "remove-down-replica",
        "replace-offline-replica",
        "make-up-replica",
        "remove-extra-replica",
        "location-replacement",
    ] {
        if schedule
            .remove(&format!("disable-{flag}"))
            .is_some_and(|v| v == "true")
        {
            schedule.insert(format!("enable-{flag}"), Value::String("false".into()));
        }
    }
}

// JSON arrays must not exploit Serde's positional struct representation.
fn check_shape(base: &Value, patch: &Value) -> Result<(), String> {
    if let Some(fields) = base.as_object() {
        if !patch.is_null() && !patch.is_object() {
            return Err("expected configuration object".into());
        }
        if let Some(values) = patch.as_object() {
            for (key, value) in values {
                if let Some(old) = fields.get(key) {
                    check_shape(old, value)?;
                }
            }
        }
    }
    Ok(())
}
