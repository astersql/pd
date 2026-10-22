// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn comparable_i64() {
    let values = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];

    let encoded: Vec<Vec<u8>> = values
        .iter()
        .map(|&value| encode_int(b"prefix".to_vec(), value))
        .collect();

    assert!(encoded.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(encoded[0], b"prefix\0\0\0\0\0\0\0\0");
    assert_eq!(encoded[3], b"prefix\x80\0\0\0\0\0\0\0");
    assert_eq!(encoded[6], b"prefix\xff\xff\xff\xff\xff\xff\xff\xff");

    for (&value, bytes) in values.iter().zip(&encoded) {
        let mut input = bytes[6..].to_vec();
        input.extend_from_slice(b"tail");
        assert_eq!(decode_int(&input), Ok((&b"tail"[..], value)));
    }

    assert_eq!(decode_int(&[0; 7]), Err(DecodeIntError::InsufficientBytes));
}
