// Copyright 2026 AsterSQL.
use crate::http_contract::json_response;
use crate::*;
use axum::{Router, extract::State, http::StatusCode, response::Response, routing::get};
use serde_json::json;
type Services = Arc<dyn PdServices>;
async fn members(State(s): State<Services>) -> Response {
    if s.healthy().await.is_err() {
        return json_response(StatusCode::SERVICE_UNAVAILABLE, &"service unavailable");
    }
    let id = s.identity();
    let member = json!({"name":id.name,"member_id":id.member_id,"peer_urls":[id.peer_url],"client_urls":[id.client_url]});
    json_response(
        StatusCode::OK,
        &json!({"header":{"cluster_id":id.cluster_id},"members":[member.clone()],"leader":member}),
    )
}
pub fn router(services: Services) -> Router {
    Router::new()
        .route("/pd/api/v1/members", get(members))
        .with_state(services)
}
