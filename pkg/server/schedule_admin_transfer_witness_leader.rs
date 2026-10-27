// Copyright 2026 AsterSQL.
//! Witness leader scheduler configuration transport boundary.
use astersql_pd_schedule_schedulers::TransferWitnessLeaderScheduler;
use axum::{body::Body, http::Request, response::Response};

/// Serve the witness leader scheduler's inherited BaseScheduler endpoint.
/// Go defines no scheduler-specific HTTP configuration: every method and path
/// returns plain text without polling the body, consuming queued heartbeats,
/// mutating state, or attempting persistence.
/// The host owns route mounting and HEAD wire-body suppression.
pub fn serve(_scheduler: &TransferWitnessLeaderScheduler, _request: Request<Body>) -> Response {
    Response::builder()
        .status(200)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from("not implements"))
        .unwrap()
}
