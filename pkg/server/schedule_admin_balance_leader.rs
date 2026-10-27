// Copyright 2026 AsterSQL.
//! Scheduler-local HTTP dispatch. The host owns synchronization and route mounting.
use astersql_pd_core::KeyRange;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{BalanceLeaderScheduler, LeaderConfigUpdate};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::Value;

fn response(status: u16, body: String, json: bool) -> Response {
    Response::builder()
        .status(status)
        .header(
            "content-type",
            if json {
                "application/json; charset=UTF-8"
            } else {
                "text/plain; charset=utf-8"
            },
        )
        .body(Body::from(body))
        .unwrap()
}
fn message(status: u16, text: &str) -> Response {
    response(status, serde_json::to_string(text).unwrap(), true)
}
/// Dispatch the path relative to this scheduler, as Go's ServeHTTP does. No global
/// server/api routes are installed. Exclusive borrowing covers update and storage I/O.
pub async fn serve(
    scheduler: &mut BalanceLeaderScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path();
    let expected = match path {
        "/config" => Method::POST,
        "/list" => Method::GET,
        _ => return response(404, "404 page not found\n".into(), false),
    };
    if *request.method() != expected {
        // Gorilla's method mismatch handler has an empty body and no Allow header.
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if path == "/list" {
        return response(
            200,
            serde_json::to_string_pretty(&scheduler.config).unwrap(),
            true,
        );
    }
    // Go ignores ReadAll's error and attempts to decode the available bytes.
    let mut bytes = Vec::new();
    let mut body = request.into_body().into_data_stream();
    while let Some(chunk) = body.next().await {
        match chunk {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }
    let patch: Value = match serde_json::from_slice(&bytes) {
        Ok(patch) => patch,
        Err(e) => return message(500, &syntax_error(&bytes, &e)),
    };
    let Some(fields) = patch.as_object() else {
        return if patch.is_null() {
            message(400, "Config item is not found.")
        } else {
            message(
                500,
                &format!(
                    "json: cannot unmarshal {} into Go value of type schedulers.balanceLeaderSchedulerParam",
                    json_type(&patch)
                ),
            )
        };
    };
    let mut candidate = scheduler.config.clone();
    // Unmarshal matches case-insensitively; FindSameFieldByJSON checks exact tags.
    let known = fields.contains_key("batch") || fields.contains_key("ranges");
    for (key, value) in fields {
        if key.eq_ignore_ascii_case("batch") && !value.is_null() {
            let Some(batch) = value.as_i64() else {
                let kind = if value.is_number() {
                    format!("number {value}")
                } else {
                    json_type(value).into()
                };
                return message(
                    500,
                    &format!(
                        "json: cannot unmarshal {kind} into Go struct field balanceLeaderSchedulerParam.batch of type int"
                    ),
                );
            };
            if batch < 1 || batch > 100 {
                return message(
                    400,
                    "invalid batch size which should be an integer between 1 and 100",
                );
            }
            candidate.set_batch(batch as usize).unwrap();
        } else if key.eq_ignore_ascii_case("ranges") {
            let ranges = if value.is_null() {
                Ok(vec![])
            } else {
                decode_ranges(value)
            };
            match ranges {
                Ok(ranges) => candidate.set_ranges(ranges),
                Err(error) => return message(500, &error),
            }
        }
    }
    let changed = serde_json::to_value(&candidate).unwrap()
        != serde_json::to_value(&scheduler.config).unwrap();
    if !changed && !known {
        return message(400, "Config item is not found.");
    }
    match scheduler
        .update_leader_config(candidate, storage, paths)
        .await
    {
        Ok(LeaderConfigUpdate::Unchanged) => {
            message(200, "Config is the same with origin, so do nothing.")
        }
        Ok(LeaderConfigUpdate::Updated { .. }) => message(200, "Config is updated."),
        Err(error) => message(500, &error.to_string()),
    }
}
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Bool(_) => "bool",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Null => "null",
    }
}
fn syntax_error(bytes: &[u8], error: &serde_json::Error) -> String {
    if error.is_eof() {
        return "unexpected end of JSON input".into();
    }
    // Decode errors at this boundary retain a useful Go-style invalid-character error.
    let position = error.column().saturating_sub(1);
    let byte = bytes.get(position).copied().unwrap_or(b'?') as char;
    format!("invalid character '{byte}' looking for beginning of value")
}
fn decode_ranges(value: &Value) -> Result<Vec<KeyRange>, String> {
    let items = value.as_array().ok_or_else(|| format!("json: cannot unmarshal {} into Go struct field balanceLeaderSchedulerParam.ranges of type []keyutil.KeyRange", json_type(value)))?;
    items
        .iter()
        .map(|item| {
            let mut result = KeyRange::default();
            if item.is_null() {
                return Ok(result);
            }
            let map = item.as_object().ok_or_else(|| {
                format!(
                    "json: cannot unmarshal {} into Go value of type map[string]string",
                    json_type(item)
                )
            })?;
            for value in map.values() {
                if !value.is_null() && !value.is_string() {
                    return Err(format!(
                        "json: cannot unmarshal {} into Go value of type string",
                        json_type(value)
                    ));
                }
            }
            let decode = |key| -> Result<Vec<u8>, String> {
                let text = map.get(key).and_then(Value::as_str).unwrap_or_default();
                let mut output = vec![];
                let mut high = None;
                for ch in text.chars() {
                    let digit = ch.to_digit(16).ok_or_else(|| {
                        format!("encoding/hex: invalid byte: U+{:04X} '{}'", ch as u32, ch)
                    })? as u8;
                    if let Some(first) = high.take() {
                        output.push(first * 16 + digit);
                    } else {
                        high = Some(digit);
                    }
                }
                if high.is_some() {
                    return Err("encoding/hex: odd length hex string".into());
                }
                Ok(output)
            };
            result.start_key = decode("start-key")?;
            result.end_key = decode("end-key")?;
            Ok(result)
        })
        .collect()
}
