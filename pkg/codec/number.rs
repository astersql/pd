// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

const SIGN_MASK: u64 = 0x8000_0000_0000_0000;
const ENCODED_I64_SIZE: usize = size_of::<i64>();

/// Errors returned when a byte sequence cannot contain an encoded integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeIntError {
    /// The input ended before a complete encoded integer was available.
    InsufficientBytes,
}

impl fmt::Display for DecodeIntError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientBytes => formatter.write_str("insufficient bytes to decode value"),
        }
    }
}

impl Error for DecodeIntError {}

/// Appends the memcomparable representation of `value` to `output`.
pub fn encode_int(mut output: Vec<u8>, value: i64) -> Vec<u8> {
    let comparable = (value as u64) ^ SIGN_MASK;
    output.extend_from_slice(&comparable.to_be_bytes());
    output
}

/// Decodes one memcomparable integer and returns its unconsumed suffix.
pub fn decode_int(input: &[u8]) -> Result<(&[u8], i64), DecodeIntError> {
    let (encoded, remaining) = input
        .split_at_checked(ENCODED_I64_SIZE)
        .ok_or(DecodeIntError::InsufficientBytes)?;
    let comparable = u64::from_be_bytes(encoded.try_into().expect("encoded i64 has fixed width"));
    let value = (comparable ^ SIGN_MASK) as i64;
    Ok((remaining, value))
}
