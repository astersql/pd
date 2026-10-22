// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

use crate::{max_start_key, min_end_key};

/// An owned byte-key range with an empty end representing positive infinity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyRange {
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
}

impl KeyRange {
    /// Copies the supplied byte boundaries into a range.
    pub fn new(start_key: impl AsRef<[u8]>, end_key: impl AsRef<[u8]>) -> Self {
        Self {
            start_key: start_key.as_ref().to_vec(),
            end_key: end_key.as_ref().to_vec(),
        }
    }

    /// Tests overlap including touching endpoints, preserving Go's empty-start rule.
    pub fn over_lapped(&self, other: &Self) -> bool {
        let left = max_start_key(&self.start_key, &other.start_key);
        let right = min_end_key(&self.end_key, &other.end_key);
        left.is_empty() || left == right || right.is_empty() || left < right
    }

    /// Tests equality of opposite boundaries, including empty byte boundaries.
    pub fn is_adjacent(&self, other: &Self) -> bool {
        self.end_key == other.start_key || other.end_key == self.start_key
    }
}

impl Serialize for KeyRange {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Go encoding/json emits map keys in lexical order.
        BTreeMap::from([
            ("end-key", encode_hex(&self.end_key)),
            ("start-key", encode_hex(&self.start_key)),
        ])
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeyRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Go unmarshals through map[string]string: missing/null values are empty,
        // and even unknown fields must have string or null values.
        let map = Option::<BTreeMap<String, Option<String>>>::deserialize(deserializer)?
            .unwrap_or_default();
        let bound = |name: &str| {
            decode_hex(map.get(name).and_then(Option::as_deref).unwrap_or_default())
                .map_err(D::Error::custom)
        };
        Ok(Self {
            start_key: bound("start-key")?,
            end_key: bound("end-key")?,
        })
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 15)]));
    }
    encoded
}

fn decode_hex(encoded: &str) -> Result<Vec<u8>, &'static str> {
    fn digit(byte: u8) -> Result<u8, &'static str> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err("invalid hexadecimal byte"),
        }
    }
    if !encoded.len().is_multiple_of(2) {
        return Err("odd hexadecimal length");
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((digit(pair[0])? << 4) | digit(pair[1])?))
        .collect()
}
