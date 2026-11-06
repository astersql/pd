// Copyright 2026 AsterSQL.
//! Explicit admission boundary for v2 services owned by migration themes 8/9.
//! A registered rejection is deliberately not a compatible domain implementation.
use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
};
use tokio_util::sync::CancellationToken;

pub(crate) fn router(cancel: CancellationToken) -> Router {
    // Installed after the existing role middleware. Ready remains local, and
    // affinity continues to use the active term's real manager in its own router.
    let router = Router::new();
    // Tonic's fallback uses HTTP 200 for gRPC errors. Unknown HTTP v2 paths must
    // not fall into it and appear to be successful JSON/domain operations.
    router
        .route("/pd/api/v2", any(not_found))
        .route("/pd/api/v2/", any(not_found))
        .route("/pd/api/v2/{*unmatched}", any(not_found))
        .with_state(cancel)
}

async fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [("content-type", "text/plain")],
        "404 page not found",
    )
        .into_response()
}
