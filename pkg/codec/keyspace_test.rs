// Copyright 2026 AsterSQL.

use crate::{
    ParseKeyspacePrefixError, RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX,
    make_keyspace_prefix, parse_keyspace_prefix,
};

#[test]
fn raw_keyspace_prefix() {
    assert_eq!(
        make_keyspace_prefix(RAW_KEYSPACE_MODE_PREFIX, 0x01_02_03),
        [b'r', 0x01, 0x02, 0x03]
    );
    assert_eq!(
        make_keyspace_prefix(TXN_KEYSPACE_MODE_PREFIX, 0xab_ff_ff_ff),
        [b'x', 0xff, 0xff, 0xff]
    );

    assert_eq!(
        parse_keyspace_prefix(&[b'r', 0x01, 0x02, 0x03]),
        Ok((RAW_KEYSPACE_MODE_PREFIX, 0x01_02_03))
    );
    assert_eq!(
        parse_keyspace_prefix(&[b'x', 0xff, 0xff, 0xff, b't']),
        Ok((TXN_KEYSPACE_MODE_PREFIX, 0xff_ff_ff))
    );
    assert_eq!(
        parse_keyspace_prefix(&[b'x', 0x01, 0x02]),
        Err(ParseKeyspacePrefixError::TooShort { actual: 3 })
    );
    assert_eq!(
        parse_keyspace_prefix(&[b't', 0x01, 0x02, 0x03]),
        Err(ParseKeyspacePrefixError::UnknownMode(b't'))
    );
}
