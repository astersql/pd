// Copyright 2026 AsterSQL.

mod bytes;

pub use bytes::{DecodeBytesError, decode_bytes, encode_bytes};

#[cfg(test)]
mod bytes_test;
