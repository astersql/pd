// Copyright 2026 AsterSQL.
use crate::{EndpointError, StorageEndpoint};
use astersql_pd_keypath::{
    keyspace_resource_group_setting_path, keyspace_resource_group_setting_prefix,
    parse_keyspace_resource_group_path, resource_group_setting_prefix,
};
use astersql_pd_storage_kv::Base;

impl StorageEndpoint {
    /// Persist opaque protobuf wire bytes supplied by the serialization adapter.
    pub async fn save_resource_group_setting(
        &self,
        keyspace_id: u32,
        name: &str,
        wire: &[u8],
    ) -> Result<(), EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .save(
                keyspace_resource_group_setting_path(keyspace_id, name).as_bytes(),
                wire,
            )
            .await?)
    }

    /// Remove exactly one setting; removing a missing setting succeeds.
    pub async fn delete_resource_group_setting(
        &self,
        keyspace_id: u32,
        name: &str,
    ) -> Result<(), EndpointError> {
        Ok(Base::new(self.store.as_ref())
            .remove(keyspace_resource_group_setting_path(keyspace_id, name).as_bytes())
            .await?)
    }

    /// Deliver legacy settings first, preserving raw names and payloads.
    pub async fn load_resource_group_settings(
        &self,
        mut callback: impl FnMut(u32, &[u8], &[u8]),
    ) -> Result<(), EndpointError> {
        self.load_rule_prefix(resource_group_setting_prefix().as_bytes(), |name, value| {
            callback(u32::MAX, name, value);
        })
        .await?;
        self.load_rule_prefix(
            keyspace_resource_group_setting_prefix().as_bytes(),
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
}
