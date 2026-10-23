// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};

/// Replication policy values before presence-sensitive adjustment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplicationModeConfig {
    #[serde(rename = "replication-mode")]
    pub replication_mode: String,
    #[serde(rename = "dr-auto-sync")]
    pub dr_auto_sync: DRAutoSyncReplicationConfig,
}

/// Auto sync values for two data centers; no replication algorithm is run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DRAutoSyncReplicationConfig {
    #[serde(rename = "label-key")]
    pub label_key: String,
    pub primary: String,
    pub dr: String,
    #[serde(rename = "primary-replicas")]
    pub primary_replicas: i64,
    #[serde(rename = "dr-replicas")]
    pub dr_replicas: i64,
    #[serde(rename = "wait-store-timeout")]
    pub wait_store_timeout: Duration,
    #[serde(rename = "wait-recover-timeout")]
    pub wait_recover_timeout: Duration,
    #[serde(rename = "pause-region-split", with = "string_bool")]
    pub pause_region_split: bool,
}

impl ReplicationModeConfig {
    /// Adjusts the mode and DR defaults using metadata rooted at this table.
    pub fn adjust(&mut self, metadata: &ConfigMetaData) {
        if !metadata.is_defined("replication-mode")
            || normalize_replication_mode(&self.replication_mode).is_empty()
        {
            self.replication_mode = "majority".into();
        }
        self.dr_auto_sync.adjust(&metadata.child(["dr-auto-sync"]));
    }
}

impl DRAutoSyncReplicationConfig {
    /// Adjusts defaults using metadata rooted at the dr-auto-sync table.
    pub fn adjust(&mut self, metadata: &ConfigMetaData) {
        if !metadata.is_defined("wait-store-timeout") {
            self.wait_store_timeout = Duration::from_nanos(60_000_000_000);
        }
    }
}

/// Returns the normalized supported mode, or an empty string for invalid input.
pub fn normalize_replication_mode(mode: &str) -> String {
    let normalized = mode.to_lowercase().replace('_', "-");
    match normalized.as_str() {
        "majority" | "dr-auto-sync" => normalized,
        _ => String::new(),
    }
}

/// Decodes a replication-mode TOML table with native booleans and nested presence.
pub fn decode_toml_replication_mode(
    input: &str,
) -> Result<(ReplicationModeConfig, ConfigMetaData), toml::de::Error> {
    let table: toml::Table = toml::from_str(input)?;
    let mut paths = Vec::new();
    for (key, value) in &table {
        paths.push(vec![key.clone()]);
        if let Some(nested) = value.as_table() {
            paths.extend(nested.keys().map(|child| vec![key.clone(), child.clone()]));
        }
    }
    let metadata = ConfigMetaData::from_defined_paths(paths);
    let mut object = serde_json::to_value(table).map_err(serde::de::Error::custom)?;
    if let Some(value) = object
        .get_mut("dr-auto-sync")
        .and_then(|dr| dr.get_mut("pause-region-split"))
    {
        let flag = value.as_bool().ok_or_else(|| {
            <toml::de::Error as serde::de::Error>::custom("expected TOML boolean")
        })?;
        *value = serde_json::Value::String(flag.to_string());
    }
    let config = serde_json::from_value(object).map_err(serde::de::Error::custom)?;
    Ok((config, metadata))
}

mod string_bool {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &bool, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(if *value { "true" } else { "false" })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(serde::de::Error::custom("expected true or false")),
        }
    }
}
