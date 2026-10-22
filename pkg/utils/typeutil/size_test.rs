// Copyright 2026 AsterSQL.

use std::str::FromStr;

use super::*;

#[test]
fn byte_size_serde_text_and_mb_fallback() {
    assert_eq!(ByteSize::from_bytes(0).to_string(), "0B");
    assert_eq!(ByteSize::from_bytes(1024).to_string(), "1KiB");
    assert_eq!(ByteSize::from_bytes(10 * 1_048_576).to_string(), "10MiB");

    let size = ByteSize::from_bytes(1_756_821_276_000);
    assert_eq!(serde_json::to_string(&size).unwrap(), r#""1.598TiB""#);
    assert_eq!(
        serde_json::from_str::<ByteSize>(r#""1.598TiB""#).unwrap(),
        ByteSize::from_bytes(1_757_019_581_186)
    );

    for text in ["10Mib", "10MiB", "10M", "10MB"] {
        assert_eq!(ByteSize::from_str(text).unwrap().as_bytes(), 10 * 1_048_576);
        assert_eq!(parse_mb_from_text(text, 2), 10);
    }
    assert_eq!(ByteSize::from_str("10i").unwrap().as_bytes(), 10);
    for text in ["10GiB", "10Gib", "10G", "10GB"] {
        assert_eq!(parse_mb_from_text(text, 2), 10 * 1_024);
    }
    for text in ["1024KiB", "1048576"] {
        assert_eq!(parse_mb_from_text(text, 2), 1);
    }
    for text in ["100KiB", "1023KiB", "1048575", "0"] {
        assert_eq!(parse_mb_from_text(text, 2), 0);
    }
    for text in ["10yiB", "10aib", "-1MiB", " 10MiB "] {
        assert_eq!(parse_mb_from_text(text, 2), 2);
    }

    assert!(serde_json::from_str::<ByteSize>("1048576").is_err());
    assert!(ByteSize::from_str("1.MiB").is_err());
    assert!(ByteSize::from_str("32m b").is_err());
}
