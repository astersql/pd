// Copyright 2026 AsterSQL.
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::controller_config_path;
use astersql_pd_keypath::{
    keyspace_resource_group_state_path, keyspace_resource_group_state_prefix,
    parse_keyspace_resource_group_path, resource_group_state_prefix,
};
use astersql_pd_storage_kv::Base;
use serde::Serialize;

impl StorageEndpoint {
    /// Encode a caller-supplied JSON persistence model and overwrite its state.
    /// Models supply Go-compatible field names and custom encoding where needed.
    pub async fn save_resource_group_states<T: Serialize + ?Sized>(
        &self,
        keyspace_id: u32,
        name: &str,
        value: &T,
    ) -> Result<(), EndpointError> {
        self.save_json(
            keyspace_resource_group_state_path(keyspace_id, name).as_bytes(),
            value,
        )
        .await
    }

    /// Remove exactly one state; removing a missing state succeeds.
    pub async fn delete_resource_group_states(
        &self,
        keyspace_id: u32,
        name: &str,
    ) -> Result<(), EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .remove(keyspace_resource_group_state_path(keyspace_id, name).as_bytes())
            .await?)
    }

    /// Deliver legacy states first, preserving raw names and payloads.
    pub async fn load_resource_group_states(
        &self,
        mut callback: impl FnMut(u32, &[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(resource_group_state_prefix().as_bytes(), |name, value| {
            callback(u32::MAX, name, value);
        })
        .await?;
        self.load_rule_prefix(
            keyspace_resource_group_state_prefix().as_bytes(),
            |suffix, value| {
                let Some(separator) = suffix.iter().position(|byte| *byte == b'/') else {
                    return;
                };
                // Only the numeric component is UTF-8; Go names are arbitrary bytes.
                let Ok(id) = std::str::from_utf8(&suffix[..=separator]) else {
                    return;
                };
                let Ok((id, _)) = parse_keyspace_resource_group_path(id) else {
                    return;
                };
                callback(id, &suffix[separator + 1..], value);
            },
        )
        .await
    }

    /// Encode a controller persistence model at the cluster-independent key.
    pub async fn save_controller_config<T: Serialize + ?Sized>(
        &self,
        value: &T,
    ) -> Result<(), EndpointError> {
        self.save_json(controller_config_path().as_bytes(), value)
            .await
    }

    /// Return the original controller bytes, including invalid JSON and non-UTF-8.
    /// Missing and empty values both return empty bytes, matching Go strings.
    pub async fn load_controller_config(&self) -> Result<Vec<u8>, EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .load(controller_config_path().as_bytes())
            .await?)
    }
}
