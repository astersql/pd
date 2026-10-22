// Copyright 2026 AsterSQL.

use serde_json::json;

use super::*;

#[test]
fn conversion_helpers() {
    let encoded = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xe8];

    assert_eq!(bytes_to_u64(&encoded), Ok(1000));
    assert_eq!(u64_to_bytes(1000), encoded);

    let error = bytes_to_u64(&encoded[..7]).unwrap_err();
    assert_eq!(error.actual_len(), 7);
    assert_eq!(error.to_string(), "invalid data, must 8 bytes, but 7");

    assert_eq!(bool_to_u64(false), 0);
    assert_eq!(bool_to_u64(true), 1);
    assert_eq!(bool_to_int(false), 0);
    assert_eq!(bool_to_int(true), 1);

    assert_eq!(json_to_u64_slice(&json!([1, 2, 3])), Some(vec![1, 2, 3]));
    assert_eq!(json_to_u64_slice(&json!([1.9, 2.1])), Some(vec![1, 2]));
    assert_eq!(json_to_u64_slice(&json!([])), Some(vec![]));
    assert_eq!(json_to_u64_slice(&json!([1, "2", 3])), None);
    assert_eq!(json_to_u64_slice(&json!({"array": [1, 2, 3]})), None);
}
