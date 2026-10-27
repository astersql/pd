// Copyright 2026 AsterSQL.
//! Label scheduler configuration transport boundary.
use astersql_pd_schedule_schedulers::LabelScheduler;
use axum::{body::Body, http::Request, response::Response};

/// Serve the label scheduler's inherited BaseScheduler endpoint.
///
/// Go has no label-specific HTTP handler: every method and relative path returns
/// plain text without reading the body, updating ranges, or accessing storage.
/// The host owns routing and HEAD wire-body suppression.
pub fn serve(_scheduler: &LabelScheduler, _request: Request<Body>) -> Response {
    Response::builder()
        .status(200)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from("not implements"))
        .unwrap()
}
