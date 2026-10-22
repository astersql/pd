// Copyright 2026 AsterSQL.

use std::{error::Error, fmt, str::FromStr, time::Duration as StdDuration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const NANOS_PER_MICROSECOND: u64 = 1_000;
const NANOS_PER_MILLISECOND: u64 = 1_000_000;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const NANOS_PER_MINUTE: u64 = 60 * NANOS_PER_SECOND;
const NANOS_PER_HOUR: u64 = 60 * NANOS_PER_MINUTE;

/// A signed nanosecond duration with Go-compatible text and JSON encoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(i64);

impl Duration {
    /// Creates a duration from a signed nanosecond count.
    pub const fn from_nanos(nanos: i64) -> Self {
        Self(nanos)
    }

    /// Returns the signed nanosecond count.
    pub const fn as_nanos(self) -> i64 {
        self.0
    }
}

/// Reports an invalid or overflowing Go duration string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseDurationError;

impl fmt::Display for ParseDurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid duration")
    }
}

impl Error for ParseDurationError {}

impl FromStr for Duration {
    type Err = ParseDurationError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_duration(text).map(Self)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_duration(self.0, formatter)
    }
}

impl Serialize for Duration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// Converts seconds to a non-negative standard duration capped at Go's
/// maximum signed nanosecond duration.
pub fn saturating_std_duration_from_seconds(seconds: i64) -> StdDuration {
    if seconds < 0 {
        return StdDuration::ZERO;
    }

    let nanos = (seconds as u64)
        .saturating_mul(NANOS_PER_SECOND)
        .min(i64::MAX as u64);
    StdDuration::from_nanos(nanos)
}

fn parse_duration(text: &str) -> Result<i64, ParseDurationError> {
    let (negative, mut remainder) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if remainder == "0" {
        return Ok(0);
    }
    if remainder.is_empty() {
        return Err(ParseDurationError);
    }

    let mut total = 0_i128;
    while !remainder.is_empty() {
        let integer_end = remainder.bytes().take_while(u8::is_ascii_digit).count();
        let mut number_end = integer_end;
        let mut fraction = "";
        if remainder.as_bytes().get(number_end) == Some(&b'.') {
            let fraction_start = number_end + 1;
            let fraction_len = remainder[fraction_start..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if integer_end == 0 && fraction_len == 0 {
                return Err(ParseDurationError);
            }
            number_end = fraction_start + fraction_len;
            fraction = &remainder[fraction_start..number_end];
        } else if integer_end == 0 {
            return Err(ParseDurationError);
        }

        let whole = if integer_end == 0 {
            0
        } else {
            remainder[..integer_end]
                .parse::<i128>()
                .map_err(|_| ParseDurationError)?
        };
        let (unit_nanos, unit_len) = parse_unit(&remainder[number_end..])?;
        let mut segment = whole.checked_mul(unit_nanos).ok_or(ParseDurationError)?;
        if !fraction.is_empty() {
            let significant_len = fraction.len().min(18);
            let significant = fraction[..significant_len]
                .parse::<i128>()
                .map_err(|_| ParseDurationError)?;
            segment = segment
                .checked_add(significant * unit_nanos / 10_i128.pow(significant_len as u32))
                .ok_or(ParseDurationError)?;
        }
        total = total.checked_add(segment).ok_or(ParseDurationError)?;
        remainder = &remainder[number_end + unit_len..];
    }

    let signed = if negative { -total } else { total };
    i64::try_from(signed).map_err(|_| ParseDurationError)
}

fn parse_unit(text: &str) -> Result<(i128, usize), ParseDurationError> {
    for (unit, nanos) in [
        ("ns", 1_i128),
        ("us", NANOS_PER_MICROSECOND as i128),
        ("µs", NANOS_PER_MICROSECOND as i128),
        ("μs", NANOS_PER_MICROSECOND as i128),
        ("ms", NANOS_PER_MILLISECOND as i128),
        ("s", NANOS_PER_SECOND as i128),
        ("m", NANOS_PER_MINUTE as i128),
        ("h", NANOS_PER_HOUR as i128),
    ] {
        if text.starts_with(unit) {
            return Ok((nanos, unit.len()));
        }
    }
    Err(ParseDurationError)
}

fn format_duration(nanos: i64, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    if nanos == 0 {
        return formatter.write_str("0s");
    }
    if nanos < 0 {
        formatter.write_str("-")?;
    }
    let nanos = nanos.unsigned_abs();
    if nanos < NANOS_PER_MICROSECOND {
        return write!(formatter, "{nanos}ns");
    }
    if nanos < NANOS_PER_MILLISECOND {
        return write_decimal_unit(formatter, nanos, NANOS_PER_MICROSECOND, 3, "µs");
    }
    if nanos < NANOS_PER_SECOND {
        return write_decimal_unit(formatter, nanos, NANOS_PER_MILLISECOND, 6, "ms");
    }

    let hours = nanos / NANOS_PER_HOUR;
    let after_hours = nanos % NANOS_PER_HOUR;
    let minutes = after_hours / NANOS_PER_MINUTE;
    let after_minutes = after_hours % NANOS_PER_MINUTE;
    if hours > 0 {
        write!(formatter, "{hours}h")?;
    }
    if hours > 0 || minutes > 0 {
        write!(formatter, "{minutes}m")?;
    }
    write_decimal_unit(formatter, after_minutes, NANOS_PER_SECOND, 9, "s")
}

fn write_decimal_unit(
    formatter: &mut fmt::Formatter<'_>,
    value: u64,
    unit: u64,
    precision: usize,
    suffix: &str,
) -> fmt::Result {
    write!(formatter, "{}", value / unit)?;
    let remainder = value % unit;
    if remainder != 0 {
        let fraction = format!("{remainder:0precision$}");
        write!(formatter, ".{}", fraction.trim_end_matches('0'))?;
    }
    formatter.write_str(suffix)
}
