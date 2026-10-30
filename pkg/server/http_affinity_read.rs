// Copyright 2026 AsterSQL.
//! Affinity reads use the host-owned manager on the existing term command queue.
use crate::{PdServices, http_contract::json_response};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
/// Preserve the encoded path and repeated query parameters until after bootstrap admission.
pub struct Request {
    pub path: String,
    pub query: String,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/affinity-groups", get(handle))
        .route("/pd/api/v2/affinity-groups/{id}", get(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
) -> Response {
    // Gin matches the decoded path before middleware. A decoded slash is no
    // longer one group_id component, even when axum matched the encoded URI.
    if uri
        .path()
        .strip_prefix("/pd/api/v2/affinity-groups/")
        .and_then(|raw| crate::http_contract::query_unescape(&raw.replace('+', "%2B")).ok())
        .is_some_and(|id| id.contains(&b'/'))
    {
        return (
            StatusCode::NOT_FOUND,
            [("content-type", "text/plain")],
            "404 page not found",
        )
            .into_response();
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        match services
            .affinity_read(Request {
                path: uri.path().into(),
                query: uri.query().unwrap_or_default().into(),
            })
            .await
        {
            Ok((code, value)) => response(StatusCode::from_u16(code).unwrap(), &value),
            Err(e) => response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>response(StatusCode::INTERNAL_SERVER_ERROR, &"context canceled"), r=work=>r}
}

fn response(status: StatusCode, value: &impl serde::Serialize) -> Response {
    let mut response = json_response(status, value);
    response.headers_mut().insert(
        "content-type",
        "application/json; charset=utf-8".parse().unwrap(),
    );
    response
}
