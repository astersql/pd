// Copyright 2026 AsterSQL.
use std::{error::Error, fmt};

use astersql_pd_errs::{TypedError, catalog::ERR_INVALID_ARGUMENT};
use serde_json::{Map, Value};

/// HTTP range errors distinguish invalid arguments from malformed URL escapes.
#[derive(Debug)]
pub enum HttpKeyRangesError {
    InvalidArgument(TypedError),
    Escape(String),
}

impl fmt::Display for HttpKeyRangesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument(error) => error.fmt(f),
            Self::Escape(escape) => write!(f, "invalid URL escape {escape:?}"),
        }
    }
}

impl Error for HttpKeyRangesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidArgument(error) => Some(error),
            Self::Escape(_) => None,
        }
    }
}

/// Decodes comma-separated HTTP start/end parameters in alternating order.
///
/// Like Go QueryUnescape, decoding preserves arbitrary bytes, converts plus to
/// space, and rejects malformed percent escapes. Bounds are split before decoding.
/// Empty end bytes retain their unbounded meaning; ranges are not validated here.
pub fn decode_http_key_ranges(
    input: &Map<String, Value>,
) -> Result<Vec<Vec<u8>>, HttpKeyRangesError> {
    let argument = |name: &str| {
        input.get(name).and_then(Value::as_str).ok_or_else(|| {
            HttpKeyRangesError::InvalidArgument(ERR_INVALID_ARGUMENT.instantiate([name]))
        })
    };
    let start = argument("start-key")?;
    let end = argument("end-key")?;
    let starts: Vec<_> = start.split(',').collect();
    let ends: Vec<_> = end.split(',').collect();
    if starts.len() != ends.len() {
        return Err(HttpKeyRangesError::InvalidArgument(
            ERR_INVALID_ARGUMENT.instantiate([start, end]),
        ));
    }
    let mut result = Vec::with_capacity(starts.len() * 2);
    for (start, end) in starts.into_iter().zip(ends) {
        result.push(query_unescape(start)?);
        result.push(query_unescape(end)?);
    }
    Ok(result)
}

fn query_unescape(input: &str) -> Result<Vec<u8>, HttpKeyRangesError> {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let digit = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
                let pair = bytes
                    .get(i + 1..i + 3)
                    .and_then(|p| Some((digit(p[0])?, digit(p[1])?)));
                let Some((high, low)) = pair else {
                    // Go reports up to three raw bytes from the invalid escape.
                    return Err(HttpKeyRangesError::Escape(
                        String::from_utf8_lossy(&bytes[i..bytes.len().min(i + 3)]).into_owned(),
                    ));
                };
                result.push(high * 16 + low);
                i += 3;
            }
            b'+' => {
                result.push(b' ');
                i += 1;
            }
            byte => {
                result.push(byte);
                i += 1;
            }
        }
    }
    Ok(result)
}
