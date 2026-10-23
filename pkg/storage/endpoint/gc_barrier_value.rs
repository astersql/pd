// Copyright 2026 AsterSQL.

use std::fmt;

use astersql_pd_typeutil::TimeOptional;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::EndpointError;

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// Wall-clock instant with nanosecond precision and Go's signed Unix-second range.
/// No platform clock, timezone or monotonic component participates in comparisons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BarrierTime(i128);

impl BarrierTime {
    /// Normalizes Unix seconds and nanoseconds, including negative offsets.
    pub const fn from_unix(seconds: i64, nanoseconds: i64) -> Self {
        Self(seconds as i128 * NANOS_PER_SECOND + nanoseconds as i128)
    }

    /// Returns floor Unix seconds, matching Go time.Time.Unix.
    pub fn unix_seconds(self) -> i64 {
        self.0.div_euclid(NANOS_PER_SECOND) as i64
    }

    fn ceil_second(self) -> Self {
        Self((self.0 + NANOS_PER_SECOND - 1).div_euclid(NANOS_PER_SECOND) * NANOS_PER_SECOND)
    }
}

/// Legacy persistent service safe point; missing keyspace means the null keyspace.
/// The zero value still denotes keyspace zero, as in the Go struct.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ServiceSafePoint {
    pub service_id: String,
    pub expired_at: i64,
    pub safe_point: u64,
    #[serde(skip_serializing_if = "is_null_keyspace")]
    pub keyspace_id: u32,
}

fn is_null_keyspace(id: &u32) -> bool {
    *id == u32::MAX
}

/// Per-keyspace GC barrier, persisted through the legacy ServiceSafePoint schema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GCBarrier {
    pub barrier_id: String,
    pub barrier_ts: u64,
    pub expiration_time: Option<BarrierTime>,
}

impl GCBarrier {
    /// Rounds expiration upward to a second and updates the caller's instant.
    /// None denotes a barrier that never expires.
    pub fn new(id: impl Into<String>, ts: u64, expiration: Option<&mut BarrierTime>) -> Self {
        Self {
            barrier_id: id.into(),
            barrier_ts: ts,
            expiration_time: round_expiration(expiration),
        }
    }

    /// Converts to the backward-compatible physical service safe point value.
    pub fn to_service_safe_point(&self, keyspace_id: u32) -> ServiceSafePoint {
        ServiceSafePoint {
            service_id: self.barrier_id.clone(),
            expired_at: self
                .expiration_time
                .map_or(i64::MAX, BarrierTime::unix_seconds),
            safe_point: self.barrier_ts,
            keyspace_id,
        }
    }

    /// Reconstructs a barrier; nonpositive or MaxInt64 expiry means no expiry.
    pub fn from_service_safe_point(value: &ServiceSafePoint) -> Self {
        Self {
            barrier_id: value.service_id.clone(),
            barrier_ts: value.safe_point,
            expiration_time: optional_time(value.expired_at),
        }
    }

    /// Expiry is strict: the exact expiration instant is still live.
    pub fn is_expired(&self, now: BarrierTime) -> bool {
        self.expiration_time
            .is_some_and(|expiration| now > expiration)
    }
}

/// Global barrier has a separate wire schema and a distinct domain type.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GlobalGCBarrier {
    pub barrier_id: String,
    pub barrier_ts: u64,
    #[serde(serialize_with = "serialize_expiration")]
    pub expiration_time: Option<BarrierTime>,
}

impl GlobalGCBarrier {
    /// Rounds expiration upward and updates the input, matching NewGlobalGCBarrier.
    pub fn new(id: impl Into<String>, ts: u64, expiration: Option<&mut BarrierTime>) -> Self {
        Self {
            barrier_id: id.into(),
            barrier_ts: ts,
            expiration_time: round_expiration(expiration),
        }
    }

    /// Expiry is strict and an absent expiration never expires.
    pub fn is_expired(&self, now: BarrierTime) -> bool {
        self.expiration_time
            .is_some_and(|expiration| now > expiration)
    }
}

fn round_expiration(expiration: Option<&mut BarrierTime>) -> Option<BarrierTime> {
    expiration.map(|time| {
        *time = time.ceil_second();
        *time
    })
}

fn optional_time(seconds: i64) -> Option<BarrierTime> {
    (seconds > 0 && seconds < i64::MAX).then(|| BarrierTime::from_unix(seconds, 0))
}

fn serialize_expiration<S: Serializer>(
    time: &Option<BarrierTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    TimeOptional(time.map(BarrierTime::unix_seconds)).serialize(serializer)
}

impl<'de> Deserialize<'de> for ServiceSafePoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = ServiceSafePoint;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a service safe point object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(ServiceSafePoint {
                    keyspace_id: u32::MAX,
                    ..Default::default()
                })
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = ServiceSafePoint {
                    keyspace_id: u32::MAX,
                    ..Default::default()
                };
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("service_id") {
                        if let Some(v) = map.next_value::<Option<String>>()? {
                            value.service_id = v;
                        }
                    } else if key.eq_ignore_ascii_case("expired_at") {
                        if let Some(v) = map.next_value::<Option<i64>>()? {
                            value.expired_at = v;
                        }
                    } else if key.eq_ignore_ascii_case("safe_point") {
                        if let Some(v) = map.next_value::<Option<u64>>()? {
                            value.safe_point = v;
                        }
                    } else if key.eq_ignore_ascii_case("keyspace_id") {
                        value.keyspace_id = map.next_value::<Option<u32>>()?.unwrap_or(u32::MAX);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for GlobalGCBarrier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = GlobalGCBarrier;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a global GC barrier object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(GlobalGCBarrier::default())
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = GlobalGCBarrier::default();
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("barrier_id") {
                        if let Some(v) = map.next_value::<Option<String>>()? {
                            value.barrier_id = v;
                        }
                    } else if key.eq_ignore_ascii_case("barrier_ts") {
                        if let Some(v) = map.next_value::<Option<u64>>()? {
                            value.barrier_ts = v;
                        }
                    } else if key.eq_ignore_ascii_case("expiration_time") {
                        value.expiration_time =
                            map.next_value::<TimeOptional>()?.0.and_then(optional_time);
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(value)
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

// Go encoding/json escapes HTML-sensitive characters and JS line separators.
fn go_json<T: Serialize>(value: &T) -> Result<Vec<u8>, EndpointError> {
    Ok(serde_json::to_string(value)
        .map_err(EndpointError::JsonMarshal)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .into_bytes())
}

macro_rules! json_codec {
    ($ty:ty) => {
        impl $ty {
            /// Encodes the exact Go persistent JSON schema, including HTML escaping.
            pub fn to_json(&self) -> Result<Vec<u8>, EndpointError> {
                go_json(self)
            }
            /// Decodes the Go persistent schema with compatible null/default handling.
            pub fn from_json(bytes: &[u8]) -> Result<Self, EndpointError> {
                serde_json::from_slice(bytes).map_err(EndpointError::JsonUnmarshal)
            }
        }
    };
}
json_codec!(ServiceSafePoint);
json_codec!(GlobalGCBarrier);
