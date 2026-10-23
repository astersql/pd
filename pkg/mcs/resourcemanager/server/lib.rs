// Copyright 2026 AsterSQL.
//! Resource controller pure configuration, without server initialization.
use astersql_pd_configutil::{ConfigMetaData, adjust_duration, adjust_float64};
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// Pure values from Go ControllerConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerConfig {
    #[serde(rename = "degraded-mode-wait-duration")]
    pub degraded_mode_wait_duration: Duration,
    #[serde(rename = "ltb-max-wait-duration")]
    pub ltb_max_wait_duration: Duration,
    #[serde(rename = "ltb-token-rpc-max-delay")]
    pub ltb_token_rpc_max_delay: Duration,
    #[serde(rename = "request-unit")]
    #[serde(deserialize_with = "null_default")]
    pub request_unit: RequestUnitConfig,
    #[serde(rename = "enable-controller-trace-log", with = "string_bool")]
    pub enable_controller_trace_log: bool,
    #[serde(rename = "push-metrics-address")]
    #[serde(deserialize_with = "null_default")]
    pub push_metrics_address: String,
    #[serde(rename = "push-metrics-interval")]
    pub push_metrics_interval: Duration,
    #[serde(rename = "ru-version-policy", skip_serializing_if = "Option::is_none")]
    pub ru_version_policy: Option<RUVersionPolicy>,
}
/// Pure values from Go RequestUnitConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestUnitConfig {
    #[serde(rename = "read-base-cost", serialize_with = "finite_float")]
    pub read_base_cost: f64,
    #[serde(rename = "read-per-batch-base-cost", serialize_with = "finite_float")]
    pub read_per_batch_base_cost: f64,
    #[serde(rename = "read-cost-per-byte", serialize_with = "finite_float")]
    pub read_cost_per_byte: f64,
    #[serde(rename = "write-base-cost", serialize_with = "finite_float")]
    pub write_base_cost: f64,
    #[serde(rename = "write-per-batch-base-cost", serialize_with = "finite_float")]
    pub write_per_batch_base_cost: f64,
    #[serde(rename = "write-cost-per-byte", serialize_with = "finite_float")]
    pub write_cost_per_byte: f64,
    #[serde(rename = "read-cpu-ms-cost", serialize_with = "finite_float")]
    pub cpu_ms_cost: f64,
}
/// Pure values from Go RUVersionPolicy; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RUVersionPolicy {
    #[serde(rename = "default")]
    #[serde(deserialize_with = "null_default")]
    pub default: i32,
    #[serde(rename = "overrides", skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub overrides: BTreeMap<u32, i32>,
}

fn finite_float<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if !v.is_finite() {
        return Err(serde::ser::Error::custom("unsupported non-finite float"));
    }
    if v.fract() == 0.0 && v.abs() < (1u64 << 53) as f64 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}
mod string_bool {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &bool, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(if *v { "true" } else { "false" })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
        match Option::<String>::deserialize(d)?.as_deref() {
            Some("true") => Ok(true),
            Some("false") | None => Ok(false),
            _ => Err(serde::de::Error::custom("expected true or false")),
        }
    }
}

impl ControllerConfig {
    /// Applies presence-sensitive cost/duration defaults; invalid push interval disables the address.
    pub fn adjust(&mut self, meta: &ConfigMetaData) {
        self.request_unit.adjust(&meta.child(["request-unit"]));
        if !meta.is_defined("degraded-mode-wait-duration") {
            adjust_duration(
                &mut self.degraded_mode_wait_duration,
                Duration::from_nanos(0),
            );
        }
        if !meta.is_defined("ltb-max-wait-duration") {
            adjust_duration(
                &mut self.ltb_max_wait_duration,
                Duration::from_nanos(30000000000),
            );
        }
        if !meta.is_defined("ltb-token-rpc-max-delay") {
            adjust_duration(
                &mut self.ltb_token_rpc_max_delay,
                Duration::from_nanos(1000000000),
            );
        }
        if !self.push_metrics_address.is_empty() && self.push_metrics_interval.as_nanos() <= 0 {
            self.push_metrics_address.clear();
        }
    }
    /// Resolves an optional policy, matching the fallback for an absent controller policy.
    pub fn keyspace_ru_version(&self, id: u32) -> i32 {
        self.ru_version_policy
            .as_ref()
            .map_or(1, |p| p.keyspace_ru_version(id))
    }
}
impl RequestUnitConfig {
    /// Adjusts only missing coefficients; explicit zero and negative costs are retained.
    pub fn adjust(&mut self, meta: &ConfigMetaData) {
        if !meta.is_defined("read-base-cost") {
            adjust_float64(&mut self.read_base_cost, 0.125);
        }
        if !meta.is_defined("read-per-batch-base-cost") {
            adjust_float64(&mut self.read_per_batch_base_cost, 0.5);
        }
        if !meta.is_defined("read-cost-per-byte") {
            adjust_float64(&mut self.read_cost_per_byte, 1.0 / 65536.0);
        }
        if !meta.is_defined("write-base-cost") {
            adjust_float64(&mut self.write_base_cost, 1.0);
        }
        if !meta.is_defined("write-per-batch-base-cost") {
            adjust_float64(&mut self.write_per_batch_base_cost, 1.0);
        }
        if !meta.is_defined("write-cost-per-byte") {
            adjust_float64(&mut self.write_cost_per_byte, 1.0 / 1024.0);
        }
        if !meta.is_defined("read-cpu-ms-cost") {
            adjust_float64(&mut self.cpu_ms_cost, 1.0 / 3.0);
        }
    }
}
impl RUVersionPolicy {
    /// Uses an override before the positive default, otherwise Go's v1 fallback.
    pub fn keyspace_ru_version(&self, id: u32) -> i32 {
        self.overrides
            .get(&id)
            .copied()
            .unwrap_or(if self.default > 0 { self.default } else { 1 })
    }
    /// Validates a policy on explicit request; ControllerConfig.Adjust does not invoke this.
    pub fn validate(&self) -> Result<(), String> {
        if self.default <= 0 {
            return Err(format!(
                "ru-version-policy default must be positive, got {}",
                self.default
            ));
        }
        for (id, v) in &self.overrides {
            if *v <= 0 {
                return Err(format!(
                    "ru-version-policy override for keyspace {id} must be positive, got {v}"
                ));
            }
        }
        Ok(())
    }
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;

/// Decodes a controller TOML table, preserving zero/negative/NaN costs and
/// rejecting integer-key overrides exactly as the Go TOML decoder does.
pub fn decode_toml(input: &str) -> Result<(ControllerConfig, ConfigMetaData), String> {
    let mut table: toml::Table = toml::from_str(input).map_err(|e| e.to_string())?;
    if table
        .get("ru-version-policy")
        .and_then(toml::Value::as_table)
        .is_some_and(|t| t.contains_key("overrides"))
    {
        return Err("toml: cannot decode to a map with non-string key type (uint32 in \"map[uint32]int32\")".into());
    }
    let mut paths = Vec::new();
    for (key, value) in &table {
        paths.push(vec![key.clone()]);
        if let Some(t) = value.as_table() {
            for child in t.keys() {
                paths.push(vec![key.clone(), child.clone()]);
            }
        }
    }
    let meta = ConfigMetaData::from_defined_paths(paths);
    let mut costs = Vec::new();
    if let Some(t) = table
        .get_mut("request-unit")
        .and_then(toml::Value::as_table_mut)
    {
        for key in [
            "read-base-cost",
            "read-per-batch-base-cost",
            "read-cost-per-byte",
            "write-base-cost",
            "write-per-batch-base-cost",
            "write-cost-per-byte",
            "read-cpu-ms-cost",
        ] {
            if let Some(v) = t.remove(key) {
                let n = match v {
                    toml::Value::Float(v) => v,
                    toml::Value::Integer(v) => v as f64,
                    _ => return Err("expected TOML number".into()),
                };
                costs.push((key, n));
            }
        }
    }
    let mut json = serde_json::to_value(table).map_err(|e| e.to_string())?;
    if let Some(v) = json.get_mut("enable-controller-trace-log") {
        let b = v.as_bool().ok_or("expected TOML boolean")?;
        *v = serde_json::Value::String(b.to_string());
    }
    let mut config: ControllerConfig = serde_json::from_value(json).map_err(|e| e.to_string())?;
    for (key, v) in costs {
        match key {
            "read-base-cost" => config.request_unit.read_base_cost = v,
            "read-per-batch-base-cost" => config.request_unit.read_per_batch_base_cost = v,
            "read-cost-per-byte" => config.request_unit.read_cost_per_byte = v,
            "write-base-cost" => config.request_unit.write_base_cost = v,
            "write-per-batch-base-cost" => config.request_unit.write_per_batch_base_cost = v,
            "write-cost-per-byte" => config.request_unit.write_cost_per_byte = v,
            _ => config.request_unit.cpu_ms_cost = v,
        }
    }
    Ok((config, meta))
}
