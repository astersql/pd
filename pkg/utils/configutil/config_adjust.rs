// Copyright 2026 AsterSQL.

use astersql_pd_typeutil::{ByteSize, Duration};

/// Replaces an empty string with the default, preserving whitespace.
pub fn adjust_string(value: &mut String, default: &str) {
    if value.is_empty() {
        *value = default.to_owned();
    }
}

/// Replaces zero with the default, preserving every nonzero value.
pub fn adjust_uint64(value: &mut u64, default: u64) {
    if *value == 0 {
        *value = default;
    }
}

/// Replaces zero with the default, preserving every nonzero value.
pub fn adjust_int64(value: &mut i64, default: i64) {
    if *value == 0 {
        *value = default;
    }
}

/// Replaces zero with the default, preserving every nonzero value.
pub fn adjust_int(value: &mut isize, default: isize) {
    if *value == 0 {
        *value = default;
    }
}

/// Replaces zero with the default, preserving every nonzero value.
pub fn adjust_float64(value: &mut f64, default: f64) {
    if *value == 0.0 {
        *value = default;
    }
}

/// Replaces a non-positive signed duration with the default.
/// The default itself is assigned unchanged, even if it is non-positive.
pub fn adjust_duration(value: &mut Duration, default: Duration) {
    if value.as_nanos() <= 0 {
        *value = default;
    }
}

/// Replaces a zero byte size with the default.
pub fn adjust_byte_size(value: &mut ByteSize, default: ByteSize) {
    if value.as_bytes() == 0 {
        *value = default;
    }
}

/// Replaces false with the default; this does not indicate field presence.
pub fn adjust_bool(value: &mut bool, default: bool) {
    if !*value {
        *value = default;
    }
}
