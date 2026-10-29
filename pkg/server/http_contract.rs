// Copyright 2026 AsterSQL.
//! Shared classic HTTP contracts. Parsing is opt-in: GET handlers must not
//! reject unused bodies, query fields or Content-Type headers.
use axum::{
    body::{Body, to_bytes},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Map, Value, json};

/// Client-safe errors; backend errors must never be used as input messages.
#[derive(Debug, PartialEq, Eq)]
pub enum HttpError {
    /// A public validation error, corresponding to apiutil's input code.
    Input(String),
    /// A body read or internal processing failure. Details stay on the server.
    Internal,
}
impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Input(message) => (StatusCode::BAD_REQUEST, "input", message),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "internal server error".into(),
            ),
        };
        let mut response = json_response(status, &json!({"code":code,"msg":message,"data":null}));
        response
            .headers_mut()
            .insert("tidb-error-code", code.parse().unwrap());
        response
    }
}

/// Render JSON with the classic Go renderer's Content-Type, including strings
/// and null. Serialization failures never expose a serializer's error text.
pub fn json_response(status: StatusCode, value: &impl Serialize) -> Response {
    let (status, bytes) = match serde_json::to_vec_pretty(value) {
        Ok(bytes) => (status, bytes),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            b"\"internal server error\"".to_vec(),
        ),
    };
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=UTF-8")],
        bytes,
    )
        .into_response()
}

/// Decode a map as ReadJSONRespondError does. Content-Type is deliberately not
/// an argument: Go reads the body regardless of its declared media type.
/// Null remains distinct from an empty map; callers decide field requirements.
pub async fn read_json_object(body: Body) -> Result<Option<Map<String, Value>>, HttpError> {
    let bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|_| HttpError::Internal)?;
    let value: Value = serde_json::from_slice(&go_json_strings(&bytes)).map_err(|error| {
        HttpError::Input(if error.is_eof() {
            "unexpected end of JSON input".into()
        } else {
            "invalid JSON input".into()
        })
    })?;
    match value {
        Value::Null => Ok(None),
        Value::Object(mut map) => {
            for value in map.values_mut() {
                go_numbers(value);
            }
            Ok(Some(map))
        }
        other => {
            let kind = match other {
                Value::Array(_) => "array",
                Value::Bool(_) => "bool",
                Value::Number(_) => "number",
                _ => "string",
            };
            Err(HttpError::Input(format!(
                "json: cannot unmarshal {kind} into Go value of type map[string]interface {{}}"
            )))
        }
    }
}

/// Parse a decimal path variable without accepting Rust's optional leading +.
/// Missing, empty, invalid and overflowing values are distinct from zero.
pub fn parse_uint64_field(value: Option<&str>, name: &str) -> Result<u64, HttpError> {
    let value = value.ok_or_else(|| HttpError::Input(format!("field {name} not present")))?;
    let syntax = value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit());
    if !syntax && let Ok(number) = value.parse() {
        return Ok(number);
    }
    let reason = if syntax {
        "invalid syntax"
    } else {
        "value out of range"
    };
    Err(HttpError::Input(format!(
        "strconv.ParseUint: parsing {}: {reason}",
        serde_json::to_string(value).unwrap()
    )))
}

/// Decode a query option exactly once. Query '+' is a space, while encoded '+'
/// stays '+'. Bytes are preserved so binary keys need not be valid UTF-8.
pub fn query_unescape(value: &str) -> Result<Vec<u8>, HttpError> {
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => result.push(b' '),
            b'%' => {
                let hex = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
                let pair = bytes
                    .get(i + 1)
                    .and_then(|a| hex(*a))
                    .zip(bytes.get(i + 2).and_then(|b| hex(*b)));
                let Some((high, low)) = pair else {
                    return Err(HttpError::Input("invalid URL escape".into()));
                };
                result.push(high * 16 + low);
                i += 2;
            }
            b => result.push(b),
        }
        i += 1;
    }
    Ok(result)
}

// encoding/json replaces invalid UTF-8 and unpaired UTF-16 surrogates inside
// strings. serde_json rejects the latter, so normalize just those escapes.
fn go_json_strings(bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut in_string = false;
    let mut i = 0;
    let unit = |at: usize| -> Option<u16> {
        if bytes.get(at..at + 2)? != b"\\u" {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(bytes.get(at + 2..at + 6)?).ok()?, 16).ok()
    };
    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_string = !in_string;
        }
        if in_string && bytes[i] == b'\\' && i + 1 < bytes.len() {
            if let Some(value) = unit(i) {
                if (0xd800..=0xdbff).contains(&value)
                    && unit(i + 6).is_some_and(|v| (0xdc00..=0xdfff).contains(&v))
                {
                    out.extend_from_slice(&bytes[i..i + 12]);
                    i += 12;
                    continue;
                }
                if (0xd800..=0xdfff).contains(&value) {
                    out.extend_from_slice(b"\\ufffd");
                    i += 6;
                    continue;
                }
            }
            out.extend_from_slice(&bytes[i..i + 2]);
            i += 2;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn go_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Some(float) = number.as_f64().and_then(serde_json::Number::from_f64) {
                *number = float;
            }
        }
        Value::Array(values) => values.iter_mut().for_each(go_numbers),
        Value::Object(values) => values.values_mut().for_each(go_numbers),
        _ => {}
    }
}
