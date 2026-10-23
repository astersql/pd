// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;
use serde::{Deserialize, Serialize};

/// Microservice switches before presence-sensitive adjustment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MicroserviceConfig {
    #[serde(rename = "enable-scheduling-fallback", with = "string_bool")]
    pub enable_scheduling_fallback: bool,
    #[serde(rename = "enable-tso-dynamic-switching", with = "string_bool")]
    pub enable_tso_dynamic_switching: bool,
    #[serde(rename = "enable-resource-manager-fallback", with = "string_bool")]
    pub enable_resource_manager_fallback: bool,
}

impl MicroserviceConfig {
    /// Applies defaults only to fields absent from metadata rooted at this table.
    pub fn adjust(&mut self, metadata: &ConfigMetaData) {
        if !metadata.is_defined("enable-scheduling-fallback") {
            self.enable_scheduling_fallback = true;
        }
        if !metadata.is_defined("enable-tso-dynamic-switching") {
            self.enable_tso_dynamic_switching = false;
        }
        if !metadata.is_defined("enable-resource-manager-fallback") {
            self.enable_resource_manager_fallback = true;
        }
    }

    /// Whether scheduling service fallback to PD is enabled.
    pub fn is_scheduling_fallback_enabled(&self) -> bool {
        self.enable_scheduling_fallback
    }

    /// Whether TSO dynamic switching is enabled.
    pub fn is_tso_dynamic_switching_enabled(&self) -> bool {
        self.enable_tso_dynamic_switching
    }

    /// Whether resource manager service fallback to the API service is enabled.
    pub fn is_resource_manager_fallback_enabled(&self) -> bool {
        self.enable_resource_manager_fallback
    }
}

/// Decodes the inner micro-service TOML table with native boolean fields.
pub fn decode_toml_microservice(
    input: &str,
) -> Result<(MicroserviceConfig, ConfigMetaData), toml::de::Error> {
    let table: toml::Table = toml::from_str(input)?;
    let metadata = ConfigMetaData::from_defined_paths(table.keys().map(|key| [key.clone()]));
    let mut object = serde_json::to_value(table).map_err(serde::de::Error::custom)?;
    for key in [
        "enable-scheduling-fallback",
        "enable-tso-dynamic-switching",
        "enable-resource-manager-fallback",
    ] {
        if let Some(value) = object.get_mut(key) {
            let flag = value.as_bool().ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom("expected TOML boolean")
            })?;
            *value = serde_json::Value::String(flag.to_string());
        }
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
        match Option::<String>::deserialize(deserializer)?.as_deref() {
            Some("true") => Ok(true),
            Some("false" | "null") | None => Ok(false),
            _ => Err(serde::de::Error::custom("expected true or false")),
        }
    }
}
