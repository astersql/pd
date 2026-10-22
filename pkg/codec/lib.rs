// Copyright 2026 AsterSQL.

mod bytes;
mod keyspace;
mod number;

pub use bytes::{DecodeBytesError, decode_bytes, encode_bytes};
pub use keyspace::{
    KEYSPACE_PREFIX_LEN, ParseKeyspacePrefixError, RAW_KEYSPACE_MODE_PREFIX,
    TXN_KEYSPACE_MODE_PREFIX, make_keyspace_prefix, parse_keyspace_prefix,
};
pub use number::{DecodeIntError, decode_int, encode_int};

#[cfg(test)]
mod bytes_test;
#[cfg(test)]
mod keyspace_test;
#[cfg(test)]
mod number_test;
