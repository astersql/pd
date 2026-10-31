// Copyright 2026 AsterSQL.
//! Classic recovery endpoint for the host-owned persistent ID allocator.
use crate::{
    PdServices,
    http_contract::{HttpError, parse_uint64_field, read_json_object},
    request_context,
};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
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
        .route("/pd/api/v1/admin/base-alloc-id", post(handle))
        .with_state(Context { services, cancel })
}
async fn handle(State(s): State<Context>, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let input = match read_json_object(body).await {
            Ok(v) => v,
            Err(e) => return e.into_response(),
        };
        let Some(id) = input
            .as_ref()
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
        else {
            return (StatusCode::BAD_REQUEST, "invalid id value").into_response();
        };
        let id = match parse_uint64_field(Some(id), "id") {
            Ok(id) => id,
            Err(HttpError::Input(e)) => return (StatusCode::BAD_REQUEST, e).into_response(),
            Err(e) => return e.into_response(),
        };
        match crate::http_admin_markers::marker(s.services.as_ref(), false, None).await {
            Ok(true) => {}
            Ok(false) => {
                return (
                    StatusCode::FORBIDDEN,
                    "can only recover alloc id when recovering mark marked",
                )
                    .into_response();
            }
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        }
        match crate::grpc_members::members(s.services.as_ref(), &cancel).await {
            Ok(r) if r.header.as_ref().is_some_and(|h| h.error.is_some()) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    r.header.unwrap().error.unwrap().message,
                )
                    .into_response();
            }
            Ok(r) if r.leader.is_none() => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "[PD:server:ErrLeaderNil]leader is nil",
                )
                    .into_response();
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.message().to_owned()).into_response();
            }
            _ => {}
        }
        match s.services.recover_alloc_id(id).await {
            Ok(()) => (StatusCode::OK, "").into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>(StatusCode::INTERNAL_SERVER_ERROR,"context canceled").into_response(), response=work=>response}
}
