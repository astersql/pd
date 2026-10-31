// Copyright 2026 AsterSQL.
//! Explicit admission boundary for v2 services owned by migration themes 8/9.
//! A registered rejection is deliberately not a compatible domain implementation.
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{MethodFilter, any, on},
};
use tokio_util::sync::CancellationToken;

pub(crate) fn router(cancel: CancellationToken) -> Router {
    // Installed after the existing role middleware. Ready remains local, and
    // affinity continues to use the active term's real manager in its own router.
    use MethodFilter as M;
    let mut router = Router::new();
    for (path, methods, domain, dependency) in [
        ("keyspaces", M::GET.or(M::POST), "keyspace", "theme-8"),
        // Gin has method-specific path trees: GET /keyspaces/id is also a name lookup.
        ("keyspaces/id", M::POST.or(M::GET), "keyspace", "theme-8"),
        ("keyspaces/{name}", M::GET, "keyspace", "theme-8"),
        ("keyspaces/{name}/config", M::PATCH, "keyspace", "theme-8"),
        ("keyspaces/{name}/state", M::PUT, "keyspace", "theme-8"),
        ("keyspaces/id/{id}", M::GET, "keyspace", "theme-8"),
        ("gc/safepoint/{keyspace_id}", M::GET, "gc", "theme-8"),
        (
            "tso/keyspace-groups",
            M::POST.or(M::GET),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "tso/keyspace-groups/{id}",
            M::GET.or(M::DELETE).or(M::PATCH),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "tso/keyspace-groups/{id}/{*node}",
            M::PATCH,
            "tso-keyspace-group",
            "theme-8",
        ),
        // PATCH catch-all remains reachable on paths that have more specific methods.
        (
            "tso/keyspace-groups/{id}/alloc",
            M::POST.or(M::PATCH),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "tso/keyspace-groups/{id}/split",
            M::POST.or(M::DELETE).or(M::PATCH),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "tso/keyspace-groups/{id}/merge",
            M::POST.or(M::DELETE).or(M::PATCH),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "tso/keyspace-groups/{id}/keyspaces",
            M::DELETE.or(M::PATCH),
            "tso-keyspace-group",
            "theme-8",
        ),
        (
            "meta-service-groups",
            M::GET.or(M::PATCH),
            "meta-service-group",
            "theme-8",
        ),
        (
            "meta-service-groups/{id}/status",
            M::PATCH,
            "meta-service-group",
            "theme-8",
        ),
        ("ms/members/{service}", M::GET, "mcs", "theme-8"),
        ("ms/primary/{service}", M::GET, "mcs", "theme-8"),
        ("ms/tso/primary/evict", M::POST, "mcs", "theme-8"),
        ("maintenance", M::GET, "maintenance", "theme-9"),
        ("maintenance/{task_type}", M::GET, "maintenance", "theme-9"),
        (
            "maintenance/{task_type}/{task_id}",
            M::PUT.or(M::DELETE),
            "maintenance",
            "theme-9",
        ),
    ] {
        router = router.route(
            &format!("/pd/api/v2/{path}"),
            on(methods, move |state| unavailable(state, domain, dependency)),
        );
    }
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

async fn unavailable(
    State(cancel): State<CancellationToken>,
    domain: &'static str,
    dependency: &'static str,
) -> Response {
    let token = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or(cancel);
    let mut response = crate::http_contract::json_response(
        StatusCode::SERVICE_UNAVAILABLE,
        &serde_json::json!({
            "error": if token.is_cancelled() { "request_cancelled" } else { "domain_dependency_unavailable" },
            "domain": domain,
            "dependency": dependency,
            "compatible": false,
        }),
    );
    response
        .headers_mut()
        .insert("x-pd-compatibility", "unavailable".parse().unwrap());
    response
}
