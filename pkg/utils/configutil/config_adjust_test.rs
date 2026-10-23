// Copyright 2026 AsterSQL.
use astersql_pd_typeutil::{ByteSize, Duration};

use super::*;

#[test]
fn config_adjust_string() {
    for (input, default, expected) in [
        ("", "fallback", "fallback"),
        ("", "", ""),
        (" ", "fallback", " "),
        ("值", "fallback", "值"),
    ] {
        let mut value = input.to_owned();
        adjust_string(&mut value, default);
        assert_eq!(value, expected);
    }
}

#[test]
fn config_adjust_integers() {
    for (input, default, expected) in [
        (0, 7, 7),
        (0, -7, -7),
        (-1, 7, -1),
        (1, 7, 1),
        (i64::MIN, 7, i64::MIN),
        (i64::MAX, 7, i64::MAX),
        (0, 0, 0),
    ] {
        let mut value = input;
        adjust_int64(&mut value, default);
        assert_eq!(value, expected);
    }
    for (input, expected) in [
        (0, 7),
        (-1, -1),
        (1, 1),
        (isize::MIN, isize::MIN),
        (isize::MAX, isize::MAX),
    ] {
        let mut value = input;
        adjust_int(&mut value, 7);
        assert_eq!(value, expected);
    }
    let mut value = 0;
    adjust_int(&mut value, -7);
    assert_eq!(value, -7);
    for (input, expected) in [(0, 7), (1, 1), (u64::MAX, u64::MAX)] {
        let mut value = input;
        adjust_uint64(&mut value, 7);
        assert_eq!(value, expected);
        let mut bytes = ByteSize::from_bytes(input);
        adjust_byte_size(&mut bytes, ByteSize::from_bytes(7));
        assert_eq!(bytes.as_bytes(), expected);
    }
}

#[test]
fn config_adjust_float64() {
    for input in [
        0.0,
        -0.0,
        1.5,
        -1.5,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_0000_0000_0042),
    ] {
        let mut value = input;
        adjust_float64(&mut value, 2.5);
        let expected = if input == 0.0 { 2.5 } else { input };
        assert_eq!(value.to_bits(), expected.to_bits());
    }
    let mut value = 0.0;
    adjust_float64(&mut value, -0.0);
    assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn config_adjust_duration() {
    for input in [i64::MIN, -1, 0, 1, i64::MAX] {
        for default in [-5, 0, 5] {
            let mut value = Duration::from_nanos(input);
            adjust_duration(&mut value, Duration::from_nanos(default));
            assert_eq!(value.as_nanos(), if input <= 0 { default } else { input });
        }
    }
}

#[test]
fn config_adjust_bool() {
    for input in [false, true] {
        for default in [false, true] {
            let mut value = input;
            adjust_bool(&mut value, default);
            assert_eq!(value, input || default);
        }
    }
}
