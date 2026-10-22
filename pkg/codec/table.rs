// Copyright 2026 AsterSQL.

use crate::{
    DecodeBytesError, KEYSPACE_PREFIX_LEN, TXN_KEYSPACE_MODE_PREFIX, decode_bytes, decode_int,
    parse_keyspace_prefix,
};

const TABLE_PREFIX: u8 = b't';
const META_PREFIX: u8 = b'm';

/// Identifies a logical table, including an API v2 keyspace when present.
///
/// Classic keys have `has_keyspace` set to false. Meta, truncated, and other
/// non-table keys use table ID zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TableIdentity {
    /// The 24-bit API v2 keyspace ID.
    pub keyspace_id: u32,
    /// The decoded TiDB table ID, or zero for a non-table key.
    pub table_id: i64,
    /// Whether the key had a recognized API v2 transactional prefix.
    pub has_keyspace: bool,
}

fn unwrap_keyspace(key: &[u8]) -> (&[u8], u32, bool) {
    let Ok((mode, keyspace_id)) = parse_keyspace_prefix(key) else {
        return (key, 0, false);
    };
    if mode != TXN_KEYSPACE_MODE_PREFIX {
        return (key, 0, false);
    }

    let payload = &key[KEYSPACE_PREFIX_LEN..];
    if !matches!(payload.first(), Some(&TABLE_PREFIX | &META_PREFIX)) {
        return (key, 0, false);
    }
    (payload, keyspace_id, true)
}

/// Returns the keyspace-qualified table identity of a memcomparable key.
pub fn table_identity(encoded_key: &[u8]) -> Result<TableIdentity, DecodeBytesError> {
    let (_, decoded) = decode_bytes(encoded_key)?;
    let (key, keyspace_id, has_keyspace) = unwrap_keyspace(&decoded);
    let table_id = match key.strip_prefix(&[TABLE_PREFIX]) {
        Some(encoded_id) => decode_int(encoded_id).map_or(0, |(_, table_id)| table_id),
        None => 0,
    };

    Ok(TableIdentity {
        keyspace_id,
        table_id,
        has_keyspace,
    })
}

/// Classifies a memcomparable key as meta, table, or neither.
///
/// Meta keys return `(true, 0)`, table keys return `(false, table_id)`, and
/// all other valid keys return `(false, 0)`.
pub fn meta_or_table(encoded_key: &[u8]) -> Result<(bool, i64), DecodeBytesError> {
    let (_, decoded) = decode_bytes(encoded_key)?;
    let (key, _, _) = unwrap_keyspace(&decoded);

    if key.starts_with(&[META_PREFIX]) {
        return Ok((true, 0));
    }
    if let Some(encoded_id) = key.strip_prefix(&[TABLE_PREFIX]) {
        let table_id = decode_int(encoded_id).map_or(0, |(_, table_id)| table_id);
        return Ok((false, table_id));
    }
    Ok((false, 0))
}
