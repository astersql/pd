// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn mcs_047_explicit_uint_saturation_and_error_text() {
    for (raw, base, value, kind) in [
        (
            b"18446744073709551615".as_slice(),
            GoUintBase::Decimal,
            u64::MAX,
            None,
        ),
        (
            b"18446744073709551616",
            GoUintBase::Decimal,
            u64::MAX,
            Some(GoUintErrorKind::Range),
        ),
        (
            b"184467440737095516150X",
            GoUintBase::Decimal,
            u64::MAX,
            Some(GoUintErrorKind::Range),
        ),
        (
            b"18446744073709551615X",
            GoUintBase::Decimal,
            0,
            Some(GoUintErrorKind::Syntax),
        ),
        (b"ffffffffffffffff", GoUintBase::Hexadecimal, u64::MAX, None),
        (
            b"10000000000000000g",
            GoUintBase::Hexadecimal,
            u64::MAX,
            Some(GoUintErrorKind::Range),
        ),
        (b"", GoUintBase::Decimal, 0, Some(GoUintErrorKind::Syntax)),
        (b"+1", GoUintBase::Decimal, 0, Some(GoUintErrorKind::Syntax)),
        (
            b"\xff",
            GoUintBase::Hexadecimal,
            0,
            Some(GoUintErrorKind::Syntax),
        ),
    ] {
        let (actual, error) = parse_uint64_with_base(raw, base);
        assert_eq!(actual, value);
        assert_eq!(error.as_ref().map(|e| e.kind), kind);
        if let Some(error) = error {
            assert_eq!(error.input, raw);
        }
    }
    assert_eq!(
        parse_uint64_with_base(b"\xff", GoUintBase::Decimal)
            .1
            .unwrap()
            .to_string(),
        "strconv.ParseUint: parsing \"\\xff\": invalid syntax"
    );
    assert_eq!(
        go_quote_bytes("终点\"\0".as_bytes(), false),
        "\"终点\\\"\\x00\""
    );
    assert_eq!(
        go_quote_bytes("最小".as_bytes(), true),
        "\"\\u6700\\u5c0f\""
    );
}
