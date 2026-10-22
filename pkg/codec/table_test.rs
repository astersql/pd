// Copyright 2026 AsterSQL.

use crate::{
    DecodeBytesError, RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX, TableIdentity,
    encode_bytes, encode_int, generate_index_key, generate_record_key_prefix, generate_row_key,
    generate_table_key, make_keyspace_prefix, meta_or_table, table_identity,
};

fn table_key(table_id: i64) -> Vec<u8> {
    encode_int(vec![b't'], table_id)
}

fn keyspace_key(mode: u8, keyspace_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut key = make_keyspace_prefix(mode, keyspace_id).to_vec();
    key.extend_from_slice(payload);
    key
}

#[test]
fn table_identity_and_meta() {
    let table_id = 100;
    let other_table_id = 200;
    let keyspace_id = 42;

    assert_eq!(
        table_identity(&encode_bytes(&table_key(table_id))),
        Ok(TableIdentity {
            keyspace_id: 0,
            table_id,
            has_keyspace: false,
        })
    );

    let txn_table = keyspace_key(TXN_KEYSPACE_MODE_PREFIX, keyspace_id, &table_key(table_id));
    let txn_other_table = keyspace_key(
        TXN_KEYSPACE_MODE_PREFIX,
        keyspace_id,
        &table_key(other_table_id),
    );
    let expected = TableIdentity {
        keyspace_id,
        table_id,
        has_keyspace: true,
    };
    assert_eq!(table_identity(&encode_bytes(&txn_table)), Ok(expected));
    assert_ne!(
        table_identity(&encode_bytes(&txn_table)),
        table_identity(&encode_bytes(&txn_other_table))
    );

    let same_table_other_keyspace = keyspace_key(
        TXN_KEYSPACE_MODE_PREFIX,
        keyspace_id + 1,
        &table_key(table_id),
    );
    assert_ne!(
        table_identity(&encode_bytes(&txn_table)),
        table_identity(&encode_bytes(&same_table_other_keyspace))
    );

    let txn_meta = keyspace_key(TXN_KEYSPACE_MODE_PREFIX, keyspace_id, b"m");
    assert_eq!(meta_or_table(&encode_bytes(&txn_meta)), Ok((true, 0)));
    assert_eq!(
        meta_or_table(&encode_bytes(&txn_table)),
        Ok((false, table_id))
    );

    let raw_table = keyspace_key(RAW_KEYSPACE_MODE_PREFIX, keyspace_id, &table_key(table_id));
    assert_eq!(
        table_identity(&encode_bytes(&raw_table)),
        Ok(TableIdentity::default())
    );
    assert_eq!(meta_or_table(&encode_bytes(&raw_table)), Ok((false, 0)));

    let ambiguous_txn = keyspace_key(TXN_KEYSPACE_MODE_PREFIX, keyspace_id, b"user");
    assert_eq!(
        table_identity(&encode_bytes(&ambiguous_txn)),
        Ok(TableIdentity::default())
    );
    assert_eq!(
        table_identity(&encode_bytes(b"t\x80\0\0\0\0\0\0")),
        Ok(TableIdentity::default())
    );
    assert_eq!(
        table_identity(&encode_bytes(b"hello")),
        Ok(TableIdentity::default())
    );
    assert_eq!(meta_or_table(&encode_bytes(b"m")), Ok((true, 0)));
    assert_eq!(meta_or_table(&encode_bytes(b"hello")), Ok((false, 0)));

    assert_eq!(
        table_identity(b"invalid"),
        Err(DecodeBytesError::InsufficientBytes)
    );
    assert_eq!(
        meta_or_table(b"invalid"),
        Err(DecodeBytesError::InsufficientBytes)
    );
}

#[test]
fn record_and_index_keys() {
    assert_eq!(
        generate_table_key(42),
        vec![b't', 0x80, 0, 0, 0, 0, 0, 0, 0x2a]
    );
    assert_eq!(
        generate_record_key_prefix(42),
        vec![b't', 0x80, 0, 0, 0, 0, 0, 0, 0x2a, b'_', b'r']
    );
    assert_eq!(
        generate_row_key(42, 1),
        vec![
            b't', 0x80, 0, 0, 0, 0, 0, 0, 0x2a, b'_', b'r', 0x80, 0, 0, 0, 0, 0, 0, 1,
        ]
    );
    assert_eq!(
        generate_index_key(42, 7),
        vec![
            b't', 0x80, 0, 0, 0, 0, 0, 0, 0x2a, b'_', b'i', 0x80, 0, 0, 0, 0, 0, 0, 7,
        ]
    );
}
