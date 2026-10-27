// Copyright 2026 AsterSQL.
//! Region scheduler configuration HTTP boundary.
use astersql_pd_schedule_schedulers::BalanceRegionScheduler;
use axum::{body::Body, http::Request, response::Response};

/// Serve the region scheduler's inherited configuration endpoint.
///
/// Go embeds BaseScheduler without overriding ServeHTTP: every method and path
/// writes this plain text response, without reading the body or changing config.
/// The host owns routing and HEAD wire-body suppression.
pub fn serve(_scheduler: &BalanceRegionScheduler, _request: Request<Body>) -> Response {
    Response::builder()
        .status(200)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from("not implements"))
        .unwrap()
}
