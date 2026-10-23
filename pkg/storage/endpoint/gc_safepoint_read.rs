// Copyright 2026 AsterSQL.

use crate::StorageEndpoint;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, StoreError};
use serde::{Deserialize, Deserializer, de};
use std::fmt;

/// Safe point syntax, JSON schema and backend failures remain distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum SafePointReadError {
    #[error("parse uint error")]
    ParseUint,
    #[error("json unmarshal: {0}")]
    JsonUnmarshal(serde_json::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl StorageEndpoint {
    /// Loads GC safe point, using legacy hex for unified GC and JSON for keyspaces.
    /// Missing and empty values yield zero; keyspace zero is distinct from unified GC.
    pub async fn load_gc_safe_point(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
    ) -> Result<u64, SafePointReadError> {
        let raw = Base::new(self.store.as_ref())
            .load(paths.gc_safe_point_path(keyspace_id).as_bytes())
            .await?;
        decode_gc_safe_point(&raw, keyspace_id)
    }
    /// Loads a decimal transaction safe point without advancing it.
    pub async fn load_txn_safe_point(
        &self,
        paths: &ClusterPaths,
        keyspace_id: u32,
    ) -> Result<u64, SafePointReadError> {
        let raw = Base::new(self.store.as_ref())
            .load(paths.txn_safe_point_path(keyspace_id).as_bytes())
            .await?;
        decode_txn_safe_point(&raw)
    }
}

pub(super) fn decode_txn_safe_point(raw: &[u8]) -> Result<u64, SafePointReadError> {
    let (value, err) = decode_txn_safe_point_parts(raw);
    match err {
        Some(err) => Err(err),
        None => Ok(value),
    }
}

// Go returns the saturated ParseUint value together with an overflow error.
// The batch API exposes both; the scalar API returns only the error.
pub(super) fn decode_txn_safe_point_parts(raw: &[u8]) -> (u64, Option<SafePointReadError>) {
    let mut value = 0u64;
    for &digit in raw {
        if !digit.is_ascii_digit() {
            return (0, Some(SafePointReadError::ParseUint));
        }
        match value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(digit - b'0')))
        {
            Some(next) => value = next,
            None => return (u64::MAX, Some(SafePointReadError::ParseUint)),
        }
    }
    (value, None)
}

pub(super) fn decode_gc_safe_point(raw: &[u8], id: u32) -> Result<u64, SafePointReadError> {
    if raw.is_empty() {
        return Ok(0);
    }
    if id == NULL_KEYSPACE_ID {
        return parse_uint(raw, 16);
    }
    serde_json::from_slice::<KeyspaceSafePoint>(raw)
        .map(|v| v.0)
        .map_err(SafePointReadError::JsonUnmarshal)
}

fn parse_uint(raw: &[u8], radix: u32) -> Result<u64, SafePointReadError> {
    // Explicit-base Go ParseUint rejects signs, prefixes, separators and whitespace.
    if !raw.iter().all(|b| {
        if radix == 16 {
            b.is_ascii_hexdigit()
        } else {
            b.is_ascii_digit()
        }
    }) {
        return Err(SafePointReadError::ParseUint);
    }
    u64::from_str_radix(
        std::str::from_utf8(raw).map_err(|_| SafePointReadError::ParseUint)?,
        radix,
    )
    .map_err(|_| SafePointReadError::ParseUint)
}

struct KeyspaceSafePoint(u64);
impl<'de> Deserialize<'de> for KeyspaceSafePoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> de::Visitor<'de> for ValueVisitor {
            type Value = KeyspaceSafePoint;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a keyspace safe point object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(KeyspaceSafePoint(0))
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut value = 0;
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("safe_point") {
                        if let Some(v) = map.next_value::<Option<u64>>()? {
                            value = v;
                        }
                    } else if key.eq_ignore_ascii_case("keyspace_id") {
                        map.next_value::<Option<u32>>()?;
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(KeyspaceSafePoint(value))
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}
