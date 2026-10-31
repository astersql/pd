// Copyright 2026 AsterSQL.
//! Process-local log threshold control; never mutates persistent configuration.
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
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/admin/log", post(change))
        .with_state(Context { services, cancel })
}
async fn change(State(s): State<Context>, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"failed to read log level",
                );
            }
        };
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    &crate::schedule_admin_balance_leader::syntax_error(&bytes, &e),
                );
            }
        };
        let level = match &value {
            serde_json::Value::String(level) => level.as_str(),
            serde_json::Value::Null => "",
            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    &format!(
                        "json: cannot unmarshal {} into Go value of type string",
                        crate::schedule_admin_balance_leader::json_type(&value)
                    ),
                );
            }
        };
        let Some(control) = s.services.log_level_control() else {
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &"host log level control unavailable",
            );
        };
        match control.set_level(level) {
            Ok(()) => json_response(StatusCode::OK, &"The log level is updated."),
            Err(error) => json_response(StatusCode::BAD_REQUEST, &error),
        }
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::INTERNAL_SERVER_ERROR, &"context canceled"),
        response = work => response,
    }
}
