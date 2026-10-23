// Copyright 2026 AsterSQL.
//! Value adapter for the pinned pingcap/log config and PD redaction. No logger installation.
use serde::{Deserialize, Serialize};
/// Pure values from Go Config; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    #[serde(rename = "level")]
    #[serde(deserialize_with = "null_default")]
    pub level: String,
    #[serde(rename = "format")]
    #[serde(deserialize_with = "null_default")]
    pub format: String,
    #[serde(rename = "disable-timestamp")]
    #[serde(deserialize_with = "null_default")]
    pub disable_timestamp: bool,
    #[serde(rename = "file")]
    #[serde(deserialize_with = "null_default")]
    pub file: FileLogConfig,
    #[serde(rename = "development")]
    #[serde(deserialize_with = "null_default")]
    pub development: bool,
    #[serde(rename = "disable-caller")]
    #[serde(deserialize_with = "null_default")]
    pub disable_caller: bool,
    #[serde(rename = "disable-stacktrace")]
    #[serde(deserialize_with = "null_default")]
    pub disable_stacktrace: bool,
    #[serde(rename = "disable-error-verbose")]
    #[serde(deserialize_with = "null_default")]
    pub disable_error_verbose: bool,
    #[serde(rename = "sampling")]
    pub sampling: Option<SamplingConfig>,
    #[serde(rename = "error-output-path")]
    #[serde(deserialize_with = "null_default")]
    pub error_output_path: String,
}
/// Pure values from Go FileLogConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileLogConfig {
    #[serde(rename = "filename")]
    #[serde(deserialize_with = "null_default")]
    pub filename: String,
    #[serde(rename = "max-size")]
    #[serde(deserialize_with = "null_default")]
    pub max_size: i64,
    #[serde(rename = "max-days")]
    #[serde(deserialize_with = "null_default")]
    pub max_days: i64,
    #[serde(rename = "max-backups")]
    #[serde(deserialize_with = "null_default")]
    pub max_backups: i64,
}

/// Serializable part of zap.SamplingConfig; hook functions remain host-owned.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplingConfig {
    #[serde(rename = "initial")]
    #[serde(deserialize_with = "null_default")]
    pub initial: i64,
    #[serde(rename = "thereafter")]
    #[serde(deserialize_with = "null_default")]
    pub thereafter: i64,
}
impl LogConfig {
    /// Applies PD's log defaults without creating a logger.
    pub fn adjust(&mut self, disable_error_verbose_defined: bool) {
        if !disable_error_verbose_defined {
            self.disable_error_verbose = true;
        }
        if self.format.is_empty() {
            self.format = "text".into();
        }
        if self.level.is_empty() {
            self.level = "info".into();
        }
    }
}

/// Log redaction's accepted values and canonical encoding.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum RedactInfoLog {
    #[default]
    Off,
    On,
    Marker,
}
impl Serialize for RedactInfoLog {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Off => s.serialize_bool(false),
            Self::On => s.serialize_bool(true),
            Self::Marker => s.serialize_str("MARKER"),
        }
    }
}
impl<'de> Deserialize<'de> for RedactInfoLog {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::Bool(false) | serde_json::Value::Null => Ok(Self::Off),
            serde_json::Value::Bool(true) => Ok(Self::On),
            serde_json::Value::String(s) if s.eq_ignore_ascii_case("MARKER") => Ok(Self::Marker),
            _ => Err(serde::de::Error::custom(
                "the \"redact-info-log\" value is invalid; it should be either false, true, or \"MARKER\"",
            )),
        }
    }
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;
