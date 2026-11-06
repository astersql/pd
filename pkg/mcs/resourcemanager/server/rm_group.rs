// Copyright 2026 AsterSQL.
//! Resource-group values independent of protobuf and transports.
use serde::{Deserialize, Serialize};

/// Token limits configured for one resource group.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenLimits {
    pub fill_rate: u64,
    pub burst_limit: i64,
    pub max_tokens: f64,
}
/// Configured bucket and its current token balance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bucket {
    pub settings: Option<TokenLimits>,
    pub tokens: f64,
}
/// Runaway-query rule, preserving enum values at the protocol boundary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Runaway {
    pub rule: Option<RunawayRule>,
    pub action: i32,
    pub watch: Option<RunawayWatch>,
    pub switch_group_name: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunawayRule {
    pub exec_elapsed_time_ms: u64,
    pub processed_keys: i64,
    pub request_unit: i64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunawayWatch {
    pub lasting_duration_ms: i64,
    pub kind: i32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Background {
    pub job_types: Vec<String>,
    pub utilization_limit: u64,
}
fn zero(v: &f64) -> bool {
    *v == 0.0
}
fn zero_u64(v: &u64) -> bool {
    *v == 0
}
/// Accumulated consumption, using the existing Go JSON persistence schema.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Consumption {
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub r_r_u: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub w_r_u: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub read_bytes: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub write_bytes: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub total_cpu_time_ms: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub sql_layer_cpu_time_ms: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub kv_read_rpc_count: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub kv_write_rpc_count: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero_u64"
    )]
    pub read_cross_az_traffic_bytes: u64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero_u64"
    )]
    pub write_cross_az_traffic_bytes: u64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub tikv_r_u_v2: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub tidb_r_u_v2: f64,
    #[serde(
        deserialize_with = "crate::null_default",
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float"
    )]
    pub tiflash_r_u_v2: f64,
}
/// Domain metadata. `ru` is absent when the Go bucket has no settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub name: String,
    pub mode: i32,
    pub ru: Option<Bucket>,
    pub ru_settings_present: bool,
    pub priority: u32,
    pub runaway: Option<Runaway>,
    pub background: Option<Background>,
    pub consumption: Option<Consumption>,
}
impl Group {
    pub fn reserved_default() -> Self {
        Self {
            name: "default".into(),
            mode: 1,
            ru_settings_present: true,
            ru: Some(Bucket {
                settings: Some(TokenLimits {
                    fill_rate: i32::MAX as u64,
                    burst_limit: -1,
                    max_tokens: 0.0,
                }),
                tokens: 0.0,
            }),
            priority: 8,
            consumption: Some(Consumption::default()),
            ..Self::default()
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 32 {
            return Err("[PD:resourcemanager:ErrInvalidGroup]invalid group settings, please check the group name".into());
        }
        if self.priority > 16 {
            return Err("[PD:resourcemanager:ErrInvalidGroup]invalid group settings, please check the group priority".into());
        }
        // The Go constructor panics for raw mode and has no usable unknown-mode bucket.
        // Keep unsupported input outside the library's usable RU implementation.
        if self.mode != 1 {
            return Err("no implementation".into());
        }
        Ok(())
    }
    pub fn initialize(&mut self) {
        self.ru = self.ru.take().filter(|b| b.settings.is_some());
        self.ru_settings_present = self.mode == 1;
        self.consumption.get_or_insert_with(Consumption::default);
    }
    /// Patch metadata and add the supplied token delta, before persistence as in Go.
    pub fn patch(&mut self, update: Self) -> Result<(), String> {
        if update.mode != self.mode {
            return Err("only support reconfigure in same mode, maybe you should delete and create a new one".into());
        }
        if update.priority > 16 {
            return Err("invalid resource group priority, the value should be in [0,16]".into());
        }
        self.priority = update.priority;
        self.runaway = update.runaway;
        self.background = update.background;
        if self.mode == 1 {
            if !update.ru_settings_present {
                return Err(
                    "invalid resource group settings, RU mode should set RU settings".into(),
                );
            }
            if let Some(delta) = update.ru {
                let bucket = self.ru.get_or_insert_with(Bucket::default);
                if delta.settings.is_some() {
                    bucket.settings = delta.settings;
                }
                bucket.tokens += delta.tokens;
            }
        }
        Ok(())
    }
}
/// Durable bucket state. Slots and loans belong to the live leader term.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BucketState {
    #[serde(
        skip_serializing_if = "zero",
        serialize_with = "crate::finite_float",
        deserialize_with = "crate::null_default"
    )]
    pub tokens: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_time"
    )]
    pub last_update: Option<String>,
    #[serde(deserialize_with = "crate::null_default")]
    pub initialized: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupState {
    #[serde(rename = "r_u", skip_serializing_if = "Option::is_none")]
    pub ru: Option<BucketState>,
    #[serde(rename = "ru_consumption", skip_serializing_if = "Option::is_none")]
    pub consumption: Option<Consumption>,
}

impl Consumption {
    pub fn accumulate(&mut self, delta: &Self) {
        self.r_r_u += delta.r_r_u;
        self.w_r_u += delta.w_r_u;
        self.read_bytes += delta.read_bytes;
        self.write_bytes += delta.write_bytes;
        self.total_cpu_time_ms += delta.total_cpu_time_ms;
        self.sql_layer_cpu_time_ms += delta.sql_layer_cpu_time_ms;
        self.kv_read_rpc_count += delta.kv_read_rpc_count;
        self.kv_write_rpc_count += delta.kv_write_rpc_count;
        self.tikv_r_u_v2 += delta.tikv_r_u_v2;
        self.tidb_r_u_v2 += delta.tidb_r_u_v2;
        self.tiflash_r_u_v2 += delta.tiflash_r_u_v2;
        self.read_cross_az_traffic_bytes = self
            .read_cross_az_traffic_bytes
            .wrapping_add(delta.read_cross_az_traffic_bytes);
        self.write_cross_az_traffic_bytes = self
            .write_cross_az_traffic_bytes
            .wrapping_add(delta.write_cross_az_traffic_bytes);
    }
}

fn deserialize_time<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(deserializer)?;
    if let Some(value) = &value {
        chrono::DateTime::parse_from_rfc3339(value).map_err(serde::de::Error::custom)?;
    }
    Ok(value)
}
