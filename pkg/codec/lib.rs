// Copyright 2026 AsterSQL.

mod bytes;
mod keyspace;
mod number;
mod table;

pub use bytes::{DecodeBytesError, decode_bytes, encode_bytes};
pub use keyspace::{
    DecodeKeyspaceKeyError, ENCODED_KEYSPACE_PREFIX_LEN, KEYSPACE_PREFIX_LEN,
    ParseKeyspacePrefixError, RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX,
    decode_keyspace_key, encode_keyspace_boundary, make_keyspace_prefix, parse_keyspace_prefix,
};
pub use number::{DecodeIntError, decode_int, encode_int};
pub use table::{
    TableIdentity, generate_index_key, generate_record_key_prefix, generate_row_key,
    generate_table_key, meta_or_table, table_identity,
};

#[cfg(test)]
mod bytes_test;
#[cfg(test)]
mod keyspace_test;
#[cfg(test)]
mod number_test;
#[cfg(test)]
mod table_test;
