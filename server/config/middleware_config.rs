// Copyright 2026 AsterSQL.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

/// Pure limit dimensions matching Go's untagged fields and 64-bit int.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MiddlewareDimensionConfig {
    #[serde(
        rename = "QPS",
        deserialize_with = "null_default",
        serialize_with = "qps_number"
    )]
    pub qps: f64,
    #[serde(rename = "QPSBurst", deserialize_with = "null_default")]
    pub qps_burst: i64,
    #[serde(rename = "ConcurrencyLimit", deserialize_with = "null_default")]
    pub concurrency_limit: u64,
}

/// Go map handle: cloning the handle shares entries, config clones copy them.
#[derive(Clone, Debug, Default)]
pub struct MiddlewareLimiterMap(Arc<RwLock<BTreeMap<String, MiddlewareDimensionConfig>>>);
impl MiddlewareLimiterMap {
    /// Replaces a dimension without constructing a runtime limiter.
    pub fn insert(&self, label: String, value: MiddlewareDimensionConfig) {
        self.0
            .write()
            .expect("middleware map poisoned")
            .insert(label, value);
    }
    /// Returns a value copy as Go map lookup does.
    pub fn get(&self, label: &str) -> Option<MiddlewareDimensionConfig> {
        self.0
            .read()
            .expect("middleware map poisoned")
            .get(label)
            .copied()
    }
    fn independent(&self) -> Self {
        Self(Arc::new(RwLock::new(
            self.0.read().expect("middleware map poisoned").clone(),
        )))
    }
}
impl Serialize for MiddlewareLimiterMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0
            .read()
            .expect("middleware map poisoned")
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for MiddlewareLimiterMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(Arc::new(RwLock::new(BTreeMap::deserialize(
            deserializer,
        )?))))
    }
}

/// Audit switch encoded as a quoted boolean.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    #[serde(rename = "enable-audit", with = "string_bool")]
    pub enable_audit: bool,
}

/// HTTP rate limit values; None represents a Go nil map.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    #[serde(rename = "enable-rate-limit", with = "string_bool")]
    pub enable_rate_limit: bool,
    #[serde(rename = "limiter-config")]
    pub limiter_config: Option<MiddlewareLimiterMap>,
}
impl Clone for RateLimitConfig {
    fn clone(&self) -> Self {
        Self {
            enable_rate_limit: self.enable_rate_limit,
            limiter_config: Some(
                self.limiter_config
                    .as_ref()
                    .map(MiddlewareLimiterMap::independent)
                    .unwrap_or_default(),
            ),
        }
    }
}

/// gRPC rate limit values with gRPC-specific JSON keys.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GRPCRateLimitConfig {
    #[serde(rename = "enable-grpc-rate-limit", with = "string_bool")]
    pub enable_rate_limit: bool,
    #[serde(rename = "grpc-limiter-config")]
    pub limiter_config: Option<MiddlewareLimiterMap>,
}
impl Clone for GRPCRateLimitConfig {
    fn clone(&self) -> Self {
        Self {
            enable_rate_limit: self.enable_rate_limit,
            limiter_config: Some(
                self.limiter_config
                    .as_ref()
                    .map(MiddlewareLimiterMap::independent)
                    .unwrap_or_default(),
            ),
        }
    }
}

/// Middleware values; top-level Clone shares maps as Go's struct copy does.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceMiddlewareConfig {
    #[serde(rename = "audit", deserialize_with = "null_default")]
    pub audit_config: AuditConfig,
    #[serde(rename = "rate-limit", deserialize_with = "null_default")]
    pub rate_limit_config: RateLimitConfig,
    #[serde(rename = "grpc-rate-limit", deserialize_with = "null_default")]
    pub grpc_rate_limit_config: GRPCRateLimitConfig,
}
impl ServiceMiddlewareConfig {
    /// Creates enabled switches and allocated maps, unlike the zero value.
    pub fn new() -> Self {
        Self {
            audit_config: AuditConfig { enable_audit: true },
            rate_limit_config: RateLimitConfig {
                enable_rate_limit: true,
                limiter_config: Some(MiddlewareLimiterMap::default()),
            },
            grpc_rate_limit_config: GRPCRateLimitConfig {
                enable_rate_limit: true,
                limiter_config: Some(MiddlewareLimiterMap::default()),
            },
        }
    }
}
impl Clone for ServiceMiddlewareConfig {
    fn clone(&self) -> Self {
        Self {
            audit_config: self.audit_config.clone(),
            rate_limit_config: RateLimitConfig {
                enable_rate_limit: self.rate_limit_config.enable_rate_limit,
                limiter_config: self.rate_limit_config.limiter_config.clone(),
            },
            grpc_rate_limit_config: GRPCRateLimitConfig {
                enable_rate_limit: self.grpc_rate_limit_config.enable_rate_limit,
                limiter_config: self.grpc_rate_limit_config.limiter_config.clone(),
            },
        }
    }
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

fn null_default<'de, T: Deserialize<'de> + Default, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

// Go encoding/json uses shortest fixed notation in [1e-6, 1e21), and an
// explicit plus sign for positive exponents. RawValue preserves number spelling.
fn qps_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("unsupported non-finite float"));
    }
    let abs = value.abs();
    let text = if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let exponential = format!("{value:e}");
        let (mantissa, exponent) = exponential.split_once('e').expect("float exponent");
        let exponent: i32 = exponent.parse().expect("numeric exponent");
        format!("{mantissa}e{exponent:+}")
    } else {
        value.to_string()
    };
    serde_json::value::RawValue::from_string(text)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}
