// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

use serde_json::Value;

const U64_BYTE_LEN: usize = size_of::<u64>();

/// Reports that a byte slice did not have the exact length required for a
/// big-endian `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BytesToU64Error {
    actual_len: usize,
}

impl BytesToU64Error {
    /// Returns the length of the rejected input.
    pub const fn actual_len(self) -> usize {
        self.actual_len
    }
}

impl fmt::Display for BytesToU64Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid data, must 8 bytes, but {}",
            self.actual_len
        )
    }
}

impl Error for BytesToU64Error {}

/// Decodes an exactly eight-byte big-endian unsigned integer.
pub fn bytes_to_u64(bytes: &[u8]) -> Result<u64, BytesToU64Error> {
    let bytes: [u8; U64_BYTE_LEN] = bytes.try_into().map_err(|_| BytesToU64Error {
        actual_len: bytes.len(),
    })?;
    Ok(u64::from_be_bytes(bytes))
}

/// Encodes an unsigned integer as exactly eight big-endian bytes.
pub const fn u64_to_bytes(value: u64) -> [u8; U64_BYTE_LEN] {
    value.to_be_bytes()
}

/// Converts `false` to zero and `true` to one.
pub const fn bool_to_u64(value: bool) -> u64 {
    value as u64
}

/// Converts `false` to zero and `true` to one using Rust's pointer-sized
/// signed integer, matching Go's `int` intent.
pub const fn bool_to_int(value: bool) -> isize {
    value as isize
}

/// Converts a JSON numeric array to unsigned integers.
///
/// JSON decoding in Go represents every number as `float64`, and the oracle
/// then applies Go's float-to-integer conversion. Reading through `as_f64`
/// preserves that path, including truncation of fractional positive values.
pub fn json_to_u64_slice(value: &Value) -> Option<Vec<u64>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_f64().map(|number| number as u64))
        .collect()
}
