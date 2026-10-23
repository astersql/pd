// Copyright 2026 AsterSQL.

use std::collections::BTreeMap;

use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};

/// Pure keyspace management values. Optional collections preserve Go nil JSON.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyspaceConfig {
    #[serde(rename = "pre-alloc")]
    pub pre_alloc: Option<Vec<String>>,
    #[serde(rename = "wait-region-split")]
    pub wait_region_split: bool,
    #[serde(rename = "wait-region-split-timeout")]
    pub wait_region_split_timeout: Duration,
    #[serde(rename = "check-region-split-interval")]
    pub check_region_split_interval: Duration,
    #[serde(rename = "meta-service-groups")]
    pub meta_service_groups: Option<BTreeMap<String, String>>,
}

impl KeyspaceConfig {
    /// Applies presence-sensitive defaults, then adjusts groups. No split is run.
    /// Metadata must be rooted at the keyspace table.
    pub fn adjust(&mut self, metadata: &ConfigMetaData) -> Result<(), String> {
        if !metadata.is_defined("wait-region-split") {
            self.wait_region_split = true;
        }
        if !metadata.is_defined("wait-region-split-timeout") {
            self.wait_region_split_timeout = Duration::from_nanos(30_000_000_000);
        }
        if !metadata.is_defined("check-region-split-interval") {
            self.check_region_split_interval = Duration::from_nanos(50_000_000);
        }
        if let Some(groups) = &mut self.meta_service_groups {
            adjust_meta_service_groups(groups)?;
        }
        Ok(())
    }

    /// Checks split timing independently of default adjustment and group checks.
    pub fn validate(&self) -> Result<(), String> {
        if self.check_region_split_interval < Duration::from_nanos(1_000_000)
            || self.check_region_split_interval > Duration::from_nanos(100_000_000)
        {
            return Err(
                "[keyspace] check-region-split-interval should between 1ms and 100ms".into(),
            );
        }
        if self.check_region_split_interval >= self.wait_region_split_timeout {
            return Err("[keyspace] check-region-split-interval should be less than wait-region-split-timeout".into());
        }
        Ok(())
    }

    /// Returns an independent preallocation list (empty when the Go value is nil).
    pub fn get_pre_alloc(&self) -> Vec<String> {
        self.pre_alloc.clone().unwrap_or_default()
    }

    /// Returns an independent group map (empty when the Go value is nil).
    pub fn get_meta_service_groups(&self) -> BTreeMap<String, String> {
        self.meta_service_groups.clone().unwrap_or_default()
    }

    /// Replaces group values without adjusting or validating them.
    pub fn set_meta_service_groups(&mut self, groups: Option<BTreeMap<String, String>>) {
        self.meta_service_groups = groups;
    }
}

/// Reports whether an ID is a single path segment; only slash is rejected.
pub fn is_valid_meta_service_group_id(id: &str) -> bool {
    !id.contains('/')
}

/// Trims and validates all groups before replacing any entry.
pub fn adjust_meta_service_groups(groups: &mut BTreeMap<String, String>) -> Result<(), String> {
    let mut normalized = BTreeMap::new();
    for (group_id, endpoint) in groups.iter() {
        let id = group_id.trim();
        if id.is_empty() {
            return Err("[keyspace] meta-service group ID cannot be empty".into());
        }
        if !is_valid_meta_service_group_id(id) {
            return Err(format!(
                "[keyspace] meta-service group ID cannot contain '/': {id}"
            ));
        }
        let address = endpoint.trim();
        if address.is_empty() {
            return Err("[keyspace] meta-service group addresses cannot be empty".into());
        }
        if normalized.contains_key(id) {
            return Err(format!(
                "[keyspace] meta-service group ID cannot be duplicated: {id}"
            ));
        }
        normalized.insert(id.to_owned(), address.to_owned());
    }
    *groups = normalized;
    Ok(())
}

/// Decodes a keyspace TOML table, retaining literal presence for adjustment.
pub fn decode_toml_keyspace(
    input: &str,
) -> Result<(KeyspaceConfig, ConfigMetaData), toml::de::Error> {
    let table: toml::Table = toml::from_str(input)?;
    let metadata = ConfigMetaData::from_defined_paths(table.keys().map(|key| [key.clone()]));
    Ok((toml::from_str(input)?, metadata))
}
