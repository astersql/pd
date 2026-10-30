// Copyright 2026 AsterSQL.
//! Checker and diagnostic reads share the term coordinator's scheduling queue.
use crate::{
    PdServices,
    http_contract::{HttpError, json_response},
};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub enum Command {
    Status(String),
    Pause(String, i64),
    Diagnostic(String),
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/checker/{name}", any(handle))
        .route("/pd/api/v1/schedulers/diagnostic/{name}", any(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    Path(name): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    if method == Method::OPTIONS {
        return StatusCode::OK.into_response();
    }
    let diagnostic = uri.path().starts_with("/pd/api/v1/schedulers/");
    if method != Method::GET && (diagnostic || method != Method::POST) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let work = async {
        let command = if diagnostic {
            Command::Diagnostic(name)
        } else if method == Method::GET {
            Command::Status(name)
        } else {
            let bytes = match axum::body::to_bytes(body, usize::MAX).await {
                Ok(v) => v,
                Err(_) => return HttpError::Internal.into_response(),
            };
            let value: Value = match serde_json::from_slice(&bytes) {
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
                    return input_error(
                        crate::schedule_admin_balance_leader::syntax_error(&bytes, &e),
                        json!({"Offset":offset}),
                    );
                }
            };
            let fields = match value {
                Value::Null => None,
                Value::Object(v) => Some(v),
                v => {
                    let kind = crate::schedule_admin_balance_leader::json_type(&v);
                    let offset = if v.is_array() || v.is_object() {
                        bytes
                            .iter()
                            .position(|b| !b.is_ascii_whitespace())
                            .unwrap_or(0)
                            + 1
                    } else {
                        bytes
                            .iter()
                            .rposition(|b| !b.is_ascii_whitespace())
                            .map_or(0, |i| i + 1)
                    };
                    return type_error(kind, "map[string]int", offset);
                }
            };
            if let Some(fields) = &fields {
                // The first type error in wire order wins, including unknown keys.
                let mut fields = fields.iter().collect::<Vec<_>>();
                fields.sort_by_key(|(k, _)| {
                    crate::http_schedule_config_write::field_bounds(&bytes, k).map(|v| v.0)
                });
                for (key, value) in fields {
                    if !value.is_null() && value.as_i64().is_none() {
                        let kind = crate::schedule_admin_balance_leader::json_type(value);
                        let kind = if value.is_number() {
                            format!("number {value}")
                        } else {
                            kind.into()
                        };
                        let (start, end) =
                            crate::http_schedule_config_write::field_bounds(&bytes, key)
                                .unwrap_or_default();
                        return type_error(
                            &kind,
                            "int",
                            if value.is_array() || value.is_object() {
                                start + 1
                            } else {
                                end
                            },
                        );
                    }
                }
            }
            let Some(delay) = fields.as_ref().and_then(|v| v.get("delay")) else {
                return json_response(StatusCode::BAD_REQUEST, &"missing pause time");
            };
            let delay = delay.as_i64().unwrap_or(0);
            if delay < 0 {
                return json_response(StatusCode::BAD_REQUEST, &"delay cannot be negative");
            }
            Command::Pause(name, delay)
        };
        match services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        match services.checker(command).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),r=work=>r}
}
/// Render the canonical pause snapshot without exposing deadline representation.
pub fn status(paused: bool) -> Value {
    json!({"paused":paused})
}

fn input_error(message: String, data: Value) -> Response {
    let mut response = json_response(
        StatusCode::BAD_REQUEST,
        &json!({"code":"input","msg":message,"data":data}),
    );
    response
        .headers_mut()
        .insert("tidb-error-code", "input".parse().unwrap());
    response
}
fn type_error(kind: &str, expected: &str, offset: usize) -> Response {
    input_error(
        format!("json: cannot unmarshal {kind} into Go value of type {expected}"),
        json!({"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}),
    )
}
