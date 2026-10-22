// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

/// The raw keyspace prefix length before memcomparable encoding.
pub const KEYSPACE_PREFIX_LEN: usize = 4;

/// The encoded keyspace boundary length after memcomparable padding.
pub const ENCODED_KEYSPACE_PREFIX_LEN: usize = 9;

const ENCODED_GROUP_SIZE: usize = 8;
const KEYSPACE_PREFIX_MARKER: u8 = 0xff - (ENCODED_GROUP_SIZE - KEYSPACE_PREFIX_LEN) as u8;
const KEYSPACE_ID_LIMIT: u32 = 1 << (8 * (KEYSPACE_PREFIX_LEN - 1));

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

/// An error returned when an encoded key does not contain a keyspace prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeKeyspaceKeyError {
    /// The key does not contain a complete encoded keyspace prefix.
    TooShort { actual: usize },
    /// The first encoded group ends before the complete keyspace prefix.
    InvalidMarker(u8),
    /// The first byte is not a supported keyspace mode.
    UnknownMode(u8),
}

impl fmt::Display for DecodeKeyspaceKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual } => write!(
                formatter,
                "encoded keyspace key is too short: expected at least {ENCODED_KEYSPACE_PREFIX_LEN} bytes, got {actual}"
            ),
            Self::InvalidMarker(marker) => {
                write!(
                    formatter,
                    "encoded keyspace marker is too small: {marker:#04x}"
                )
            }
            Self::UnknownMode(mode) => {
                write!(formatter, "unknown keyspace mode byte: {mode:#04x}")
            }
        }
    }
}

impl Error for DecodeKeyspaceKeyError {}

/// Constructs the raw keyspace prefix for `mode` and the low 24 bits of `id`.
pub fn make_keyspace_prefix(mode: u8, id: u32) -> [u8; KEYSPACE_PREFIX_LEN] {
    [mode, (id >> 16) as u8, (id >> 8) as u8, id as u8]
}

/// Encodes the fixed-width memcomparable boundary for a keyspace ID.
///
/// IDs outside the 24-bit keyspace range become the exclusive fencepost for
/// the mode by incrementing its byte and resetting the encoded ID to zero.
pub fn encode_keyspace_boundary(mut mode: u8, mut id: u32) -> [u8; ENCODED_KEYSPACE_PREFIX_LEN] {
    if id >= KEYSPACE_ID_LIMIT {
        mode = mode.wrapping_add(1);
        id = 0;
    }

    [
        mode,
        (id >> 16) as u8,
        (id >> 8) as u8,
        id as u8,
        0,
        0,
        0,
        0,
        KEYSPACE_PREFIX_MARKER,
    ]
}

/// Extracts the API mode and keyspace ID from a memcomparable encoded key.
pub fn decode_keyspace_key(key: &[u8]) -> Result<(u8, u32), DecodeKeyspaceKeyError> {
    if key.len() < ENCODED_KEYSPACE_PREFIX_LEN {
        return Err(DecodeKeyspaceKeyError::TooShort { actual: key.len() });
    }
    if key[ENCODED_GROUP_SIZE] < KEYSPACE_PREFIX_MARKER {
        return Err(DecodeKeyspaceKeyError::InvalidMarker(
            key[ENCODED_GROUP_SIZE],
        ));
    }

    let mode = key[0];
    if mode != RAW_KEYSPACE_MODE_PREFIX && mode != TXN_KEYSPACE_MODE_PREFIX {
        return Err(DecodeKeyspaceKeyError::UnknownMode(mode));
    }

    let id = u32::from_be_bytes([0, key[1], key[2], key[3]]);
    Ok((mode, id))
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

/// Returns the supported raw and transactional modes in Go oracle order.
pub const fn keyspace_modes() -> [u8; 2] {
    [RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX]
}
