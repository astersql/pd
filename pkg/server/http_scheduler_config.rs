// Copyright 2026 AsterSQL.
//! Mount scheduler-local handlers; the host serializes access to live policies.
use crate::{PdServices, http_contract::json_response};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/scheduler-config", any(handle))
        .route("/pd/api/v1/scheduler-config/", any(handle))
        .route("/pd/api/v1/scheduler-config/{*path}", any(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    mut request: Request<Body>,
) -> Response {
    *request.uri_mut() = uri;
    if request.method() == axum::http::Method::OPTIONS {
        return StatusCode::OK.into_response();
    }
    if !matches!(
        *request.method(),
        axum::http::Method::GET
            | axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::DELETE
            | axum::http::Method::PATCH
    ) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let work = async {
        match services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::NOT_ACCEPTABLE,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::NOT_ACCEPTABLE, &e),
        }
        match services.scheduler_config(request).await {
            Ok(r) => r,
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),r=work=>r}
}
/// Match Go's plain HTTP fallback, including nosniff.
pub fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [
            ("content-type", "text/plain; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        "404 page not found\n",
    )
        .into_response()
}
