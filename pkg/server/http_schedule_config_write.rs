// Copyright 2026 AsterSQL.
//! Schedule writes use the host's serialized coordinator transaction.
use crate::{PdServices, http_contract::json_response, request_context};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::StatusCode,
    response::Response,
    routing::post,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Write {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
async fn write(State(s): State<Write>, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(v) => v,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        let patch: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    &crate::schedule_admin_balance_leader::syntax_error(&bytes, &e),
                );
            }
        };
        if !patch.is_object() && !patch.is_null() {
            return json_response(
                StatusCode::BAD_REQUEST,
                &format!(
                    "json: cannot unmarshal {} into Go value of type map[string]interface {{}}",
                    crate::schedule_admin_balance_leader::json_type(&patch)
                ),
            );
        }
        if let Some(response) = validate_types(&patch, &bytes) {
            return response;
        }
        match s.services.patch_schedule(patch).await {
            Ok(()) => json_response(StatusCode::OK, &"The config is updated."),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::INTERNAL_SERVER_ERROR, &"[PD:server:ErrServerNotStarted]server not started"),
        result = work => result,
    }
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/config/schedule", post(write))
        .with_state(Write { services, cancel })
}

// Decode against the canonical field types before entering the serialized write.
fn validate_types(patch: &serde_json::Value, bytes: &[u8]) -> Option<Response> {
    use serde_json::json;
    let fields = patch.as_object()?;
    let defaults = serde_json::to_value(astersql_pd_server_config::Config::default()).ok()?;
    for (key, value) in fields {
        if value.is_null() {
            continue;
        }
        let string_bool = matches!(
            defaults["schedule"]
                .get(key)
                .and_then(serde_json::Value::as_str),
            Some("true" | "false")
        ) || matches!(
            key.as_str(),
            "halt-scheduling"
                | "disable-raft-learner"
                | "disable-remove-down-replica"
                | "disable-replace-offline-replica"
                | "disable-make-up-replica"
                | "disable-remove-extra-replica"
                | "disable-location-replacement"
        );
        if string_bool && value != "true" && value != "false" {
            let detail = match value.as_str() {
                Some(value) => format!("{value:?}"),
                None => "unquoted value".into(),
            };
            return Some(json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!(
                    "json: invalid use of ,string struct tag, trying to unmarshal {detail} into bool"
                ),
            ));
        }
        if key == "store-limit"
            && let Some(limits) = value.as_object()
        {
            for limit in limits.values() {
                if !limit.is_null() && !limit.is_object() {
                    let kind = crate::schedule_admin_balance_leader::json_type(limit);
                    let offset = if limit.is_array() {
                        1
                    } else {
                        serde_json::to_vec(limit).unwrap().len()
                    };
                    let mut response = json_response(
                        StatusCode::BAD_REQUEST,
                        &json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go value of type config.StoreLimitConfig"),"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}}),
                    );
                    response
                        .headers_mut()
                        .insert("tidb-error-code", "input".parse().unwrap());
                    return Some(response);
                }
            }
        }
        let decoded = if key == "default-store-limit" && !value.is_object() {
            Err("expected config.StoreLimitConfig".into())
        } else if key == "schedulers-v2"
            && value.as_array().is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| !entry.is_null() && !entry.is_object())
            })
        {
            Err("expected config.SchedulerConfig".into())
        } else {
            serde_json::from_value::<astersql_pd_server_config::Config>(
                json!({"schedule":{key:value}}),
            )
            .map_err(|error| error.to_string())
        };
        if let Err(error) = decoded {
            if error == "invalid duration" {
                return Some(json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &duration_error(value.as_str().unwrap_or_default()),
                ));
            }
            let expected = if error == "expected config.StoreLimitConfig" {
                Some("config.StoreLimitConfig")
            } else if error == "expected config.SchedulerConfig" {
                Some("config.SchedulerConfig")
            } else if error.contains("expected u64") {
                Some("uint64")
            } else if error.contains("expected f64") {
                Some("float64")
            } else if error.contains("expected i64") {
                Some("int64")
            } else if error.contains("expected a string") || error.contains("expected string") {
                Some("string")
            } else {
                None
            };
            let Some(expected) = expected else {
                return Some(json_response(StatusCode::INTERNAL_SERVER_ERROR, &error));
            };
            let kind = crate::schedule_admin_balance_leader::json_type(value);
            let kind = if value.is_number() {
                format!("number {value}")
            } else {
                kind.into()
            };
            let Some((value_start, value_end)) = field_bounds(bytes, key) else {
                // Failure to locate diagnostic metadata must never allow an invalid patch.
                return Some(json_response(StatusCode::INTERNAL_SERVER_ERROR, &error));
            };
            let offset = if value.is_array() || value.is_object() {
                value_start + 1
            } else {
                value_end
            };
            let field = format!("scheduleConfig.{key}");
            let mut response = json_response(
                StatusCode::BAD_REQUEST,
                &json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go struct field .{field} of type {expected}"),"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":field}}),
            );
            response
                .headers_mut()
                .insert("tidb-error-code", "input".parse().unwrap());
            return Some(response);
        }
    }
    None
}

fn duration_error(value: &str) -> String {
    let invalid = || format!("time: invalid duration {value:?}");
    let mut rest = value.strip_prefix(['+', '-']).unwrap_or(value);
    while !rest.is_empty() {
        let length = rest
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b'.')
            .count();
        if length == 0 {
            return invalid();
        }
        rest = &rest[length..];
        if rest.is_empty() {
            return format!("time: missing unit in duration {value:?}");
        }
        let length = rest
            .char_indices()
            .find(|(_, c)| c.is_ascii_digit() || *c == '.')
            .map_or(rest.len(), |(i, _)| i);
        let unit = &rest[..length];
        if !matches!(unit, "ns" | "us" | "µs" | "μs" | "ms" | "s" | "m" | "h") {
            return format!("time: unknown unit {unit:?} in duration {value:?}");
        }
        rest = &rest[length..];
    }
    invalid()
}

// Locate decoded keys rather than searching their spelling: JSON permits escaped
// field names and arbitrary whitespace. Nested values are consumed as one token.
fn field_bounds(bytes: &[u8], wanted: &str) -> Option<(usize, usize)> {
    let mut cursor = bytes.iter().position(|b| *b == b'{')? + 1;
    loop {
        cursor += bytes
            .get(cursor..)?
            .iter()
            .take_while(|b| b.is_ascii_whitespace() || **b == b',')
            .count();
        let mut key = serde_json::Deserializer::from_slice(bytes.get(cursor..)?)
            .into_iter::<serde_json::Value>();
        let name = key.next()?.ok()?;
        cursor += key.byte_offset();
        cursor += bytes
            .get(cursor..)?
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
        if *bytes.get(cursor)? != b':' {
            return None;
        }
        cursor += 1;
        cursor += bytes
            .get(cursor..)?
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count();
        let start = cursor;
        let mut value = serde_json::Deserializer::from_slice(bytes.get(cursor..)?)
            .into_iter::<serde_json::Value>();
        value.next()?.ok()?;
        cursor += value.byte_offset();
        if name.as_str() == Some(wanted) {
            return Some((start, cursor));
        }
    }
}
