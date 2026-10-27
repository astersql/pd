// Copyright 2026 AsterSQL.
//! Scheduler configuration persistence over the host's canonical MetaStore endpoint.
use crate::{SchedulerError, decode_config, encode_config};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::StorageEndpoint;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::sync::Arc;

/// Typed scheduler configuration. The schema owns field validation and defaults.
/// Updates require exclusive access and publish only after a successful save.
/// Separate instances have last-writer-wins storage semantics, not a global CAS.
pub struct SchedulerConfig<T> {
    name: String,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    data: T,
}
impl<T> SchedulerConfig<T> {
    pub fn new<S: AsRef<StorageEndpoint> + ?Sized>(
        name: String,
        storage: Arc<S>,
        paths: ClusterPaths,
        data: T,
    ) -> Self {
        let storage = Arc::new(storage.as_ref().as_ref().clone());
        Self {
            name,
            storage,
            paths,
            data,
        }
    }
    pub fn get(&self) -> &T {
        &self.data
    }
}
impl<T: Serialize> SchedulerConfig<T> {
    /// Persist the current value; serialization failure never reaches storage.
    pub async fn save(&self) -> Result<(), SchedulerError> {
        let bytes = encode_config(&self.data)?;
        self.storage
            .save_scheduler_config(&self.paths, &self.name, &bytes)
            .await?;
        Ok(())
    }
    /// Commit the candidate before publishing it in this instance.
    pub async fn update(&mut self, candidate: T) -> Result<(), SchedulerError> {
        let bytes = encode_config(&candidate)?;
        self.storage
            .save_scheduler_config(&self.paths, &self.name, &bytes)
            .await?;
        self.data = candidate;
        Ok(())
    }
}
impl<T: DeserializeOwned> SchedulerConfig<T> {
    /// Missing/empty, invalid or schema-incompatible data returns a decode error.
    /// Any failure retains the previous value.
    pub async fn reload(&mut self) -> Result<(), SchedulerError> {
        let bytes = self
            .storage
            .load_scheduler_config(&self.paths, &self.name)
            .await?;
        let candidate = decode_config(&bytes)?;
        self.data = candidate;
        Ok(())
    }
}
impl<T: Serialize + DeserializeOwned> SchedulerConfig<T> {
    /// Decode a complete replacement using the scheduler's schema, then persist.
    /// Schemas should use deny_unknown_fields when unknown fields are errors.
    pub async fn update_json(&mut self, bytes: &[u8]) -> Result<(), SchedulerError> {
        self.update(decode_config(bytes)?).await
    }
}

/// Persisted disabled state; the complete config is retained when toggling disabled.
/// Loads refresh state; load failures retain the last local state, as in Go.
pub struct DefaultSchedulerConfig {
    name: String,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    data: Value,
    disabled: bool,
}
impl DefaultSchedulerConfig {
    pub fn new<S: AsRef<StorageEndpoint> + ?Sized>(
        name: String,
        storage: Arc<S>,
        paths: ClusterPaths,
        data: Value,
    ) -> Self {
        let storage = Arc::new(storage.as_ref().as_ref().clone());
        let disabled = data
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self {
            name,
            storage,
            paths,
            data,
            disabled,
        }
    }
    pub async fn is_disabled(&mut self) -> bool {
        if let Ok(bytes) = self
            .storage
            .load_scheduler_config(&self.paths, &self.name)
            .await
        {
            // Go leaves absent/null fields untouched and ignores unknown fields.
            #[derive(Deserialize)]
            struct Disabled {
                disabled: Option<bool>,
            }
            if let Ok(config) = decode_config::<Disabled>(&bytes) {
                if let Some(disabled) = config.disabled {
                    self.disabled = disabled;
                }
            }
        }
        self.disabled
    }
    pub async fn set_disabled(&mut self, disabled: bool) -> Result<(), SchedulerError> {
        self.disabled = disabled; // Deliberately retained even when persistence fails.
        if !self.data.is_object() {
            self.data = Value::Object(Default::default());
        }
        self.data["disabled"] = Value::Bool(disabled);
        let data = encode_config(&self.data)?;
        self.storage
            .save_scheduler_config(&self.paths, &self.name, &data)
            .await?;
        Ok(())
    }
}
