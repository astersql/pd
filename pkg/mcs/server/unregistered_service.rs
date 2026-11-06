// Copyright 2026 AsterSQL.
//! Protocol fallback only; no domain service is installed for imported descriptors.
use axum::{
    extract::Request,
    http::{StatusCode, Version},
    response::{IntoResponse, Response},
};
/// Match grpc-go's unknown-service response without swallowing ordinary HTTP 404s.
/// Install only on the tonic route set; embedding hosts retain their own HTTP routes.
pub async fn unregistered_service(request: Request) -> Response {
    let content = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if request.version() != Version::HTTP_2
        || !(content == "application/grpc"
            || content.starts_with("application/grpc+")
            || content.starts_with("application/grpc;"))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let method = request
        .uri()
        .path()
        .strip_prefix('/')
        .unwrap_or(request.uri().path());
    let message = match method.rsplit_once('/') {
        Some((service, _)) => format!("unknown service {service}"),
        None => format!("malformed method name: {method:?}"),
    };
    tonic::Status::unimplemented(message).into_http()
}
