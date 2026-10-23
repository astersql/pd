// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;
use serde::{Deserialize, Deserializer, Serialize};

/// Dashboard values without filesystem access or TLS construction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardConfig {
    #[serde(rename = "tidb-cacert-path", deserialize_with = "null_default")]
    pub tidb_ca_path: String,
    #[serde(rename = "tidb-cert-path", deserialize_with = "null_default")]
    pub tidb_cert_path: String,
    #[serde(rename = "tidb-key-path", deserialize_with = "null_default")]
    pub tidb_key_path: String,
    #[serde(rename = "public-path-prefix", deserialize_with = "null_default")]
    pub public_path_prefix: String,
    #[serde(rename = "temp-dir", deserialize_with = "null_default")]
    pub temp_dir: String,
    #[serde(rename = "internal-proxy", deserialize_with = "null_default")]
    pub internal_proxy: bool,
    #[serde(rename = "enable-telemetry", deserialize_with = "null_default")]
    pub enable_telemetry: bool,
    #[serde(rename = "enable-experimental", deserialize_with = "null_default")]
    pub enable_experimental: bool,
    #[serde(rename = "disable-custom-prom-addr", deserialize_with = "null_default")]
    pub disable_custom_prom_addr: bool,
}

impl DashboardConfig {
    /// Applies the current Go default (telemetry disabled) to missing fields.
    pub fn adjust(&mut self, metadata: &ConfigMetaData) {
        self.adjust_with_telemetry_default(metadata, false);
    }

    /// Applies a host-provided telemetry default only when the field is absent.
    /// Metadata is rooted at the dashboard table; no global edition is installed.
    pub fn adjust_with_telemetry_default(&mut self, metadata: &ConfigMetaData, default: bool) {
        if !metadata.is_defined("enable-telemetry") {
            self.enable_telemetry = default;
        }
    }
}

/// Decodes the inner dashboard TOML table, preserving field presence.
pub fn decode_toml_dashboard(
    input: &str,
) -> Result<(DashboardConfig, ConfigMetaData), toml::de::Error> {
    let table: toml::Table = toml::from_str(input)?;
    let metadata = ConfigMetaData::from_defined_paths(table.keys().map(|key| [key.clone()]));
    Ok((toml::from_str(input)?, metadata))
}

// Go encoding/json leaves fresh primitive fields at zero for JSON null.
fn null_default<'de, T: Deserialize<'de> + Default, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
