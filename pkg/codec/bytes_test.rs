// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn memcomparable_bytes() {
    let cases: &[(&[u8], &[u8])] = &[
        (&[], &[0, 0, 0, 0, 0, 0, 0, 0, 0xf7]),
        (&[1, 2, 3], &[1, 2, 3, 0, 0, 0, 0, 0, 0xfa]),
        (
            &[1, 2, 3, 4, 5, 6, 7, 8],
            &[1, 2, 3, 4, 5, 6, 7, 8, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0xf7],
        ),
    ];

    for &(plain, encoded) in cases {
        assert_eq!(encode_bytes(plain), encoded);
        assert_eq!(decode_bytes(encoded), Ok((&[][..], plain.to_vec())));
    }

    let mut with_leftover = encode_bytes(b"abc");
    with_leftover.extend_from_slice(b"tail");
    assert_eq!(
        decode_bytes(&with_leftover),
        Ok((&b"tail"[..], b"abc".to_vec()))
    );

    assert_eq!(
        decode_bytes(&[0; 8]),
        Err(DecodeBytesError::InsufficientBytes)
    );

    let mut invalid_marker = [0; 9];
    invalid_marker[8] = 0xf6;
    assert_eq!(
        decode_bytes(&invalid_marker),
        Err(DecodeBytesError::InvalidMarker)
    );

    let mut invalid_padding = [0; 9];
    invalid_padding[7] = 1;
    invalid_padding[8] = 0xf7;
    assert_eq!(
        decode_bytes(&invalid_padding),
        Err(DecodeBytesError::InvalidPadding)
    );
}
