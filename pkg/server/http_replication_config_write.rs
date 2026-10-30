// Copyright 2026 AsterSQL.
//! Replication configuration writes on the shared HTTP listener.
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
    labels: bool,
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
                let offset = if e.is_eof() {
                    bytes.len()
                } else {
                    bytes
                        .split_inclusive(|b| *b == b'\n')
                        .take(e.line().saturating_sub(1))
                        .map(<[u8]>::len)
                        .sum::<usize>()
                        + e.column()
                };
                return tagged(
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({"code":"input","msg":crate::schedule_admin_balance_leader::syntax_error(&bytes,&e),"data":{"Offset":offset}}),
                );
            }
        };
        // Go dereferences a nil *ReplicationConfig for JSON null; its recovery
        // middleware returns this 500 heading. Preserve the observable failure
        // without unwinding a Rust worker or exposing an implementation stack.
        if !s.labels && patch.is_null() {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("content-type", "text/plain; charset=utf-8")
                .body(Body::from(
                    "PANIC: runtime error: invalid memory address or nil pointer dereference\n",
                ))
                .unwrap();
        }
        if let Some(response) = validate(&patch, &bytes, s.labels) {
            return response;
        }
        match s.services.patch_replication(s.labels, patch).await {
            Ok(()) => json_response(StatusCode::OK, &"The config is updated."),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased; _=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"[PD:server:ErrServerNotStarted]server not started"), result=work=>result}
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/config/replicate", post(write))
        .with_state(Write {
            services: services.clone(),
            cancel: cancel.clone(),
            labels: false,
        })
        .merge(
            Router::new()
                .route("/pd/api/v1/config/label-property", post(write))
                .with_state(Write {
                    services,
                    cancel,
                    labels: true,
                }),
        )
}
fn validate(patch: &serde_json::Value, bytes: &[u8], labels: bool) -> Option<Response> {
    if patch.is_null() {
        return None;
    }
    let Some(fields) = patch.as_object() else {
        let kind = crate::schedule_admin_balance_leader::json_type(patch);
        let expected = if labels {
            "map[string]string"
        } else {
            "config.ReplicationConfig"
        };
        let offset = if patch.is_array() {
            bytes
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or_default()
                + 1
        } else {
            bytes
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map_or(0, |i| i + 1)
        };
        return Some(tagged(
            StatusCode::BAD_REQUEST,
            serde_json::json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go value of type {expected}"),"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}}),
        ));
    };
    for (key, value) in fields {
        if !labels && key == "location-labels" && !value.is_string() {
            return Some(internal("invalid syntax"));
        }
        if value.is_null() {
            continue;
        }
        let expected = if labels {
            Some("string")
        } else {
            match key.as_str() {
                "max-replicas" => Some("uint64"),
                "location-labels" => Some("typeutil.StringSlice"),
                "isolation-level" => Some("string"),
                "strictly-match-label"
                | "enable-placement-rules"
                | "enable-placement-rules-cache" => {
                    if value != "true" && value != "false" {
                        let detail = value
                            .as_str()
                            .map(|v| format!("{v:?}"))
                            .unwrap_or_else(|| "unquoted value".into());
                        return Some(internal(&format!(
                            "json: invalid use of ,string struct tag, trying to unmarshal {detail} into bool"
                        )));
                    }
                    None
                }
                _ => None,
            }
        };
        let Some(expected) = expected else {
            continue;
        };
        let valid = match expected {
            "string" => value.is_string(),
            "uint64" => value.as_u64().is_some(),
            _ => serde_json::from_value::<astersql_pd_server_config::Config>(
                serde_json::json!({"replication":{key:value}}),
            )
            .is_ok(),
        };
        if valid {
            continue;
        }
        let kind = crate::schedule_admin_balance_leader::json_type(value);
        let kind = if value.is_number() && expected == "uint64" {
            format!("number {value}")
        } else {
            kind.into()
        };
        let Some((start, end)) = crate::http_schedule_config_write::field_bounds(bytes, key) else {
            return Some(internal("invalid JSON field location"));
        };
        let offset = if value.is_array() || value.is_object() {
            start + 1
        } else {
            end
        };
        let field = if labels { String::new() } else { key.clone() };
        let (structure, message) = if labels {
            (
                "",
                format!("json: cannot unmarshal {kind} into Go value of type {expected}"),
            )
        } else {
            (
                "ReplicationConfig",
                format!(
                    "json: cannot unmarshal {kind} into Go struct field ReplicationConfig.{key} of type {expected}"
                ),
            )
        };
        let mut response = json_response(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({"code":"input","msg":message,"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":structure,"Field":field}}),
        );
        response
            .headers_mut()
            .insert("tidb-error-code", "input".parse().unwrap());
        return Some(response);
    }
    None
}

fn internal(message: &str) -> Response {
    tagged(
        StatusCode::INTERNAL_SERVER_ERROR,
        serde_json::json!({"code":"internal","msg":message,"data":{},"stack":[]}),
    )
}
fn tagged(status: StatusCode, value: serde_json::Value) -> Response {
    let mut response = json_response(status, &value);
    response.headers_mut().insert(
        "tidb-error-code",
        value["code"].as_str().unwrap().parse().unwrap(),
    );
    response
}
