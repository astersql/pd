// Copyright 2026 AsterSQL.

use std::{error::Error, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const TIB: u64 = 1 << 40;
const PIB: u64 = 1 << 50;

const BINARY_ABBREVIATIONS: [&str; 9] =
    ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];

/// A byte count with Go-compatible human-readable text and JSON encoding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(u64);

impl ByteSize {
    /// Creates a byte size from an unsigned byte count.
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Returns the underlying byte count.
    pub const fn as_bytes(self) -> u64 {
        self.0
    }
}

/// Reports an invalid or overflowing human-readable byte size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseByteSizeError;

impl fmt::Display for ParseByteSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid size")
    }
}

impl Error for ParseByteSizeError {}

impl FromStr for ByteSize {
    type Err = ParseByteSizeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse_bytes(text).map(Self)
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut unit_index = 0;
        let mut size = self.0 as f64;
        while size >= 1024.0 && unit_index < BINARY_ABBREVIATIONS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        let integer_digits = if size >= 1000.0 {
            4
        } else if size >= 100.0 {
            3
        } else if size >= 10.0 {
            2
        } else {
            1
        };
        let precision = 4_usize.saturating_sub(integer_digits);
        let mut formatted = format!("{size:.precision$}");
        if formatted.contains('.') {
            let trimmed_len = formatted.trim_end_matches('0').trim_end_matches('.').len();
            formatted.truncate(trimmed_len);
        }
        write!(
            formatter,
            "{}{}",
            formatted, BINARY_ABBREVIATIONS[unit_index]
        )
    }
}

impl Serialize for ByteSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ByteSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// Parses a human-readable byte size and returns whole mebibytes.
///
/// Invalid text returns `fallback`; valid sub-mebibyte values truncate to zero.
pub fn parse_mb_from_text(text: &str, fallback: u64) -> u64 {
    text.parse::<ByteSize>()
        .map_or(fallback, |size| size.as_bytes() / MIB)
}

fn parse_bytes(text: &str) -> Result<u64, ParseByteSizeError> {
    let bytes = text.as_bytes();
    let number_end = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b'.')
        .count();
    if number_end == 0 || !bytes[0].is_ascii_digit() {
        return Err(ParseByteSizeError);
    }
    if text[..number_end].split('.').any(str::is_empty) {
        return Err(ParseByteSizeError);
    }

    let number = text[..number_end]
        .parse::<f64>()
        .map_err(|_| ParseByteSizeError)?;
    if !number.is_finite() {
        return Err(ParseByteSizeError);
    }

    let mut suffix = &text[number_end..];
    if let Some(without_space) = suffix.strip_prefix(' ') {
        suffix = without_space;
    }
    let suffix = suffix.to_ascii_lowercase();
    let multiplier = match suffix.as_str() {
        "" | "b" | "i" | "ib" => 1,
        "k" | "kb" | "ki" | "kib" => KIB,
        "m" | "mb" | "mi" | "mib" => MIB,
        "g" | "gb" | "gi" | "gib" => GIB,
        "t" | "tb" | "ti" | "tib" => TIB,
        "p" | "pb" | "pi" | "pib" => PIB,
        _ => return Err(ParseByteSizeError),
    };
    let size = number * multiplier as f64;
    if size > i64::MAX as f64 {
        return Err(ParseByteSizeError);
    }
    Ok((size as i64) as u64)
}
