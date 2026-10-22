// Copyright 2026 AsterSQL.

use std::time::Duration;

const FLOAT64_EQUAL_TOLERANCE: f64 = 1e-6;

/// Returns the smaller of two unsigned 64-bit integers.
pub fn min_u64(a: u64, b: u64) -> u64 {
    a.min(b)
}

/// Returns the larger of two unsigned 64-bit integers.
pub fn max_u64(a: u64, b: u64) -> u64 {
    a.max(b)
}

/// Returns the shorter of two durations.
pub fn min_duration(a: Duration, b: Duration) -> Duration {
    a.min(b)
}

/// Reports whether two floating-point values differ by at most `1e-6`.
pub fn float64_equal(a: f64, b: f64) -> bool {
    (a - b).abs() <= FLOAT64_EQUAL_TOLERANCE
}

/// Reports whether two string slices contain the same values with the same
/// multiplicities, regardless of order.
pub fn are_string_slices_equivalent(a: &[String], b: &[String]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut a = a.iter().collect::<Vec<_>>();
    let mut b = b.iter().collect::<Vec<_>>();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// Reports whether two base URLs are equal after removing an HTTP(S) scheme.
pub fn equal_base_urls(url1: &str, url2: &str) -> bool {
    trim_scheme(url1) == trim_scheme(url2)
}

/// Removes a leading lowercase `https://` or `http://` scheme.
pub fn trim_scheme(value: &str) -> &str {
    value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value)
}

#[cfg(test)]
mod comparison_test;
