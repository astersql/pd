// Copyright 2026 AsterSQL.

use crate::{
    DecodeKeyspaceKeyError, ParseKeyspacePrefixError, RAW_KEYSPACE_MODE_PREFIX,
    TXN_KEYSPACE_MODE_PREFIX, decode_keyspace_key, encode_bytes, encode_keyspace_boundary,
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

#[test]
fn encoded_keyspace_boundary() {
    for mode in [RAW_KEYSPACE_MODE_PREFIX, TXN_KEYSPACE_MODE_PREFIX] {
        let boundary = encode_keyspace_boundary(mode, 0x01_02_03);
        assert_eq!(boundary.as_slice(), encode_bytes(&[mode, 0x01, 0x02, 0x03]));
        assert_eq!(decode_keyspace_key(&boundary), Ok((mode, 0x01_02_03)));

        let encoded_key = encode_bytes(&[mode, 0x01, 0x02, 0x03, b'k']);
        assert_eq!(decode_keyspace_key(&encoded_key), Ok((mode, 0x01_02_03)));
    }

    let maximum_id_fencepost = 1 << 24;
    assert_eq!(
        encode_keyspace_boundary(RAW_KEYSPACE_MODE_PREFIX, maximum_id_fencepost).as_slice(),
        encode_bytes(&[b's', 0, 0, 0])
    );
    assert_eq!(
        encode_keyspace_boundary(TXN_KEYSPACE_MODE_PREFIX, maximum_id_fencepost).as_slice(),
        encode_bytes(&[b'y', 0, 0, 0])
    );

    assert_eq!(
        decode_keyspace_key(&[]),
        Err(DecodeKeyspaceKeyError::TooShort { actual: 0 })
    );
    assert_eq!(
        decode_keyspace_key(&[b'z', 0, 0, 0, 0, 0, 0, 0, 0xfb]),
        Err(DecodeKeyspaceKeyError::UnknownMode(b'z'))
    );
    assert_eq!(
        decode_keyspace_key(&[b'r', 0, 0, 0, 0, 0, 0, 0, 0xfa]),
        Err(DecodeKeyspaceKeyError::InvalidMarker(0xfa))
    );
}
