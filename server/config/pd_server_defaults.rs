// Copyright 2026 AsterSQL.

use astersql_pd_configutil::{ConfigMetaData, adjust_duration, adjust_float64, adjust_int64};
use astersql_pd_typeutil::{Duration, StringSlice};
use serde::{Deserialize, Serialize};

/// PD server values before adjustment, with Go JSON field encodings.
/// This does not enable region storage or a GC tuner. Use decode_toml_pd_server
/// for TOML input, which has native boolean and array types.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PDServerConfig {
    #[serde(rename = "use-region-storage", with = "string_bool")]
    pub use_region_storage: bool,
    #[serde(rename = "max-gap-reset-ts")]
    pub max_reset_ts_gap: Duration,
    #[serde(rename = "key-type")]
    pub key_type: String,
    #[serde(rename = "runtime-services")]
    pub runtime_services: StringSlice,
    #[serde(rename = "metric-storage")]
    pub metric_storage: String,
    #[serde(rename = "dashboard-address")]
    pub dashboard_address: String,
    #[serde(rename = "flow-round-by-digit")]
    pub flow_round_by_digit: i64,
    #[serde(rename = "min-resolved-ts-persistence-interval")]
    pub min_resolved_ts_persistence_interval: Duration,
    #[serde(rename = "server-memory-limit", serialize_with = "finite_float")]
    pub server_memory_limit: f64,
    #[serde(
        rename = "server-memory-limit-gc-trigger",
        serialize_with = "finite_float"
    )]
    pub server_memory_limit_gc_trigger: f64,
    #[serde(rename = "enable-gogc-tuner", with = "string_bool")]
    pub enable_gogc_tuner: bool,
    #[serde(rename = "gc-tuner-threshold", serialize_with = "finite_float")]
    pub gc_tuner_threshold: f64,
    #[serde(rename = "block-safe-point-v1", with = "string_bool")]
    pub block_safe_point_v1: bool,
    #[serde(rename = "gc-keyspace-index-load-concurrency")]
    pub gc_keyspace_index_load_concurrency: i64,
    #[serde(rename = "gc-state-cache-warmup-concurrency")]
    pub gc_state_cache_warmup_concurrency: i64,
}

impl PDServerConfig {
    /// Applies only the value-defaulting portion of Go PDServerConfig.adjust.
    /// Pass metadata rooted at pd-server. Validation and deprecated-key migration
    /// are separate operations; this method makes no claim that values are valid.
    pub fn adjust_defaults(&mut self, metadata: &ConfigMetaData) {
        if !metadata.is_defined("gc-keyspace-index-load-concurrency") {
            adjust_int64(&mut self.gc_keyspace_index_load_concurrency, 4);
        }
        if !metadata.is_defined("gc-state-cache-warmup-concurrency") {
            adjust_int64(&mut self.gc_state_cache_warmup_concurrency, 4);
        }
        adjust_duration(
            &mut self.max_reset_ts_gap,
            Duration::from_nanos(86_400_000_000_000),
        );
        if !metadata.is_defined("use-region-storage") {
            self.use_region_storage = true;
        }
        if !metadata.is_defined("key-type") {
            self.key_type = "table".into();
        }
        if !metadata.is_defined("runtime-services") {
            self.runtime_services = StringSlice::default();
        }
        if !metadata.is_defined("dashboard-address") {
            self.dashboard_address = "auto".into();
        }
        if !metadata.is_defined("flow-round-by-digit") {
            adjust_int64(&mut self.flow_round_by_digit, 3);
        }
        if !metadata.is_defined("min-resolved-ts-persistence-interval") {
            adjust_duration(
                &mut self.min_resolved_ts_persistence_interval,
                Duration::from_nanos(1_000_000_000),
            );
        }
        if !metadata.is_defined("server-memory-limit") {
            adjust_float64(&mut self.server_memory_limit, 0.0);
        }
        self.server_memory_limit = self.server_memory_limit.clamp(0.0, 0.99);
        if !metadata.is_defined("server-memory-limit-gc-trigger") {
            adjust_float64(&mut self.server_memory_limit_gc_trigger, 0.7);
        }
        self.server_memory_limit_gc_trigger = self.server_memory_limit_gc_trigger.clamp(0.5, 0.99);
        if !metadata.is_defined("enable-gogc-tuner") {
            self.enable_gogc_tuner = true;
        }
        if !metadata.is_defined("gc-tuner-threshold") {
            adjust_float64(&mut self.gc_tuner_threshold, 0.6);
        }
        self.gc_tuner_threshold = self.gc_tuner_threshold.clamp(0.0, 0.9);
    }
}

/// Decodes a PDServerConfig TOML table (without the enclosing pd-server header).
/// Returns literal key presence for adjust_defaults; does not validate values.
pub fn decode_toml_pd_server(
    input: &str,
) -> Result<(PDServerConfig, ConfigMetaData), toml::de::Error> {
    let mut table: toml::Table = toml::from_str(input)?;
    let metadata = ConfigMetaData::from_defined_paths(table.keys().map(|key| [key.clone()]));
    // TOML permits NaN and infinities. Extract these fields before the JSON
    // representation, which would turn non-finite numbers into null.
    let mut floats = Vec::new();
    for key in [
        "server-memory-limit",
        "server-memory-limit-gc-trigger",
        "gc-tuner-threshold",
    ] {
        if let Some(value) = table.remove(key) {
            let value = match value {
                toml::Value::Float(value) => value,
                toml::Value::Integer(value) => value as f64,
                _ => {
                    return Err(<toml::de::Error as serde::de::Error>::custom(
                        "expected TOML number",
                    ));
                }
            };
            floats.push((key, value));
        }
    }
    let mut object = serde_json::to_value(table).map_err(serde::de::Error::custom)?;
    let fields = object.as_object_mut().expect("TOML table is an object");
    for key in [
        "use-region-storage",
        "enable-gogc-tuner",
        "block-safe-point-v1",
    ] {
        if let Some(value) = fields.get_mut(key) {
            let flag = value.as_bool().ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom("expected TOML boolean")
            })?;
            *value = serde_json::Value::String(flag.to_string());
        }
    }
    let services: Option<Vec<String>> = fields
        .remove("runtime-services")
        .map(serde_json::from_value)
        .transpose()
        .map_err(serde::de::Error::custom)?;
    let mut config: PDServerConfig =
        serde_json::from_value(object).map_err(serde::de::Error::custom)?;
    // TOML arrays may contain commas within an element; avoid a JSON string
    // roundtrip that would split those elements.
    if let Some(services) = services {
        config.runtime_services = services.into();
    }
    for (key, value) in floats {
        match key {
            "server-memory-limit" => config.server_memory_limit = value,
            "server-memory-limit-gc-trigger" => config.server_memory_limit_gc_trigger = value,
            _ => config.gc_tuner_threshold = value,
        }
    }
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

fn finite_float<S: serde::Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("unsupported non-finite float"));
    }
    if *value == 0.0 && value.is_sign_positive() {
        serializer.serialize_u64(0)
    } else {
        serializer.serialize_f64(*value)
    }
}
