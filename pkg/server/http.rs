// Copyright 2026 AsterSQL.
use crate::*;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
type Services = Arc<dyn PdServices>;
async fn health(State(s): State<Services>) -> (StatusCode, Json<Value>) {
    let id = s.identity();
    match s.healthy().await {
        Ok(()) => (
            StatusCode::OK,
            Json(
                json!([{ "name":id.name, "member_id":id.member_id, "client_urls":[id.client_url], "health":true }]),
            ),
        ),
        Err(error) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!(error))),
    }
}
async fn members(State(s): State<Services>) -> (StatusCode, Json<Value>) {
    if let Err(error) = s.healthy().await {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!(error)));
    }
    let id = s.identity();
    let member = json!({"name":id.name,"member_id":id.member_id,"peer_urls":[id.peer_url],"client_urls":[id.client_url]});
    (
        StatusCode::OK,
        Json(
            json!({"header":{"cluster_id":id.cluster_id},"members":[member.clone()],"leader":member}),
        ),
    )
}
pub fn router(services: Services) -> Router {
    Router::new()
        .route("/pd/api/v1/health", get(health))
        .route("/pd/api/v1/members", get(members))
        .with_state(services)
}
