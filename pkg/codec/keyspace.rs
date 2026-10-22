// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

/// The raw keyspace prefix length before memcomparable encoding.
pub const KEYSPACE_PREFIX_LEN: usize = 4;

/// The raw API keyspace mode byte.
pub const RAW_KEYSPACE_MODE_PREFIX: u8 = b'r';

/// The transactional API keyspace mode byte.
pub const TXN_KEYSPACE_MODE_PREFIX: u8 = b'x';

/// An error returned when a raw keyspace prefix cannot be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseKeyspacePrefixError {
    /// The key does not contain the complete four-byte prefix.
    TooShort { actual: usize },
    /// The first byte is not a supported keyspace mode.
    UnknownMode(u8),
}

impl fmt::Display for ParseKeyspacePrefixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(
                formatter,
                "keyspace prefix is too short: expected at least {KEYSPACE_PREFIX_LEN} bytes, got {actual}"
            ),
            Self::UnknownMode(mode) => {
                write!(formatter, "unknown keyspace mode byte: {mode:#04x}")
            }
        }
    }
}

impl Error for ParseKeyspacePrefixError {}

/// Constructs the raw keyspace prefix for `mode` and the low 24 bits of `id`.
pub fn make_keyspace_prefix(mode: u8, id: u32) -> [u8; KEYSPACE_PREFIX_LEN] {
    [mode, (id >> 16) as u8, (id >> 8) as u8, id as u8]
}

/// Parses a raw keyspace prefix, ignoring any payload after the first four bytes.
pub fn parse_keyspace_prefix(key: &[u8]) -> Result<(u8, u32), ParseKeyspacePrefixError> {
    if key.len() < KEYSPACE_PREFIX_LEN {
        return Err(ParseKeyspacePrefixError::TooShort { actual: key.len() });
    }

    let mode = key[0];
    if mode != RAW_KEYSPACE_MODE_PREFIX && mode != TXN_KEYSPACE_MODE_PREFIX {
        return Err(ParseKeyspacePrefixError::UnknownMode(mode));
    }

    let id = u32::from_be_bytes([0, key[1], key[2], key[3]]);
    Ok((mode, id))
}
