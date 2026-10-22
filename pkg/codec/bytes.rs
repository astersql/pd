// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

const GROUP_SIZE: usize = 8;
const MARKER: u8 = 0xff;
const PAD: u8 = 0;

/// Errors returned when a byte sequence is not valid memcomparable encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeBytesError {
    /// The input ended before a complete encoded group was available.
    InsufficientBytes,
    /// The group marker represents more than eight padding bytes.
    InvalidMarker,
    /// A padding byte in the terminating group was not zero.
    InvalidPadding,
}

impl fmt::Display for DecodeBytesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientBytes => formatter.write_str("insufficient bytes to decode value"),
            Self::InvalidMarker => formatter.write_str("invalid marker byte"),
            Self::InvalidPadding => formatter.write_str("invalid padding byte"),
        }
    }
}

impl Error for DecodeBytesError {}

/// Encodes bytes using the memcomparable representation used by PD.
pub fn encode_bytes(data: &[u8]) -> Vec<u8> {
    let group_count = data.len() / GROUP_SIZE + 1;
    let mut encoded = Vec::with_capacity(group_count * (GROUP_SIZE + 1));

    for group_start in (0..=data.len()).step_by(GROUP_SIZE) {
        let remaining = data.len() - group_start;
        let pad_count = if remaining >= GROUP_SIZE {
            encoded.extend_from_slice(&data[group_start..group_start + GROUP_SIZE]);
            0
        } else {
            encoded.extend_from_slice(&data[group_start..]);
            let pad_count = GROUP_SIZE - remaining;
            encoded.resize(encoded.len() + pad_count, PAD);
            pad_count
        };
        encoded.push(MARKER - pad_count as u8);
    }

    encoded
}

/// Decodes one memcomparable byte sequence and returns its unconsumed suffix.
pub fn decode_bytes(input: &[u8]) -> Result<(&[u8], Vec<u8>), DecodeBytesError> {
    let mut remaining = input;
    let mut decoded = Vec::with_capacity(input.len());

    loop {
        if remaining.len() < GROUP_SIZE + 1 {
            return Err(DecodeBytesError::InsufficientBytes);
        }

        let (group, rest) = remaining.split_at(GROUP_SIZE);
        let marker = rest[0];
        let pad_count = MARKER - marker;
        if pad_count as usize > GROUP_SIZE {
            return Err(DecodeBytesError::InvalidMarker);
        }

        let real_group_size = GROUP_SIZE - pad_count as usize;
        decoded.extend_from_slice(&group[..real_group_size]);
        remaining = &rest[1..];

        if pad_count != 0 {
            if group[real_group_size..].iter().any(|&byte| byte != PAD) {
                return Err(DecodeBytesError::InvalidPadding);
            }
            return Ok((remaining, decoded));
        }
    }
}
