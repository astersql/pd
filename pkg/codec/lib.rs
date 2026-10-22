// Copyright 2026 AsterSQL.

mod bytes;
mod number;

pub use bytes::{DecodeBytesError, decode_bytes, encode_bytes};
pub use number::{DecodeIntError, decode_int, encode_int};

#[cfg(test)]
mod bytes_test;
#[cfg(test)]
mod number_test;
