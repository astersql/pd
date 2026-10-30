// Copyright 2026 AsterSQL.
//! Leader discovery and host-owned consensus leadership transfer.
use crate::{PdServices, grpc_members, http_contract::json_response, request_context};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Leader {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
impl Leader {
    fn token(&self) -> CancellationToken {
        request_context::current()
            .map(|c| c.cancel)
            .unwrap_or_else(|| self.cancel.child_token())
    }
}
async fn get_leader(State(s): State<Leader>) -> Response {
    let cancel = s.token();
    match grpc_members::members(s.services.as_ref(), &cancel).await {
        Err(e) => json_response(StatusCode::SERVICE_UNAVAILABLE, &e.message()),
        Ok(r) => {
            if let Some(e) = r.header.and_then(|h| h.error) {
                return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.message);
            }
            json_response(
                StatusCode::OK,
                &r.leader.as_ref().map(crate::http_member::member_json),
            )
        }
    }
}
async fn transfer(s: Leader, name: Option<String>) -> Response {
    let cancel = s.token();
    let work = async {
        let target = if let Some(name) = &name {
            let response = match grpc_members::members(s.services.as_ref(), &cancel).await {
                Ok(r) => r,
                Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.message()),
            };
            if let Some(e) = response.header.and_then(|h| h.error) {
                return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.message);
            }
            match response.members.iter().find(|m| &m.name == name) {
                Some(member) => Some(member.member_id),
                None => {
                    return json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &"no valid pd to transfer etcd leader",
                    );
                }
            }
        } else {
            None
        };
        match s.services.transfer_leader(target).await {
            Ok(()) => json_response(
                StatusCode::OK,
                &if name.is_some() {
                    "The transfer command is submitted."
                } else {
                    "The resign command is submitted."
                },
            ),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"),
        response = work => response,
    }
}
async fn resign(State(s): State<Leader>) -> Response {
    transfer(s, None).await
}
async fn to_member(State(s): State<Leader>, Path(name): Path<String>) -> Response {
    transfer(s, Some(name)).await
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/leader", get(get_leader))
        .route("/pd/api/v1/leader/resign", post(resign))
        .route("/pd/api/v1/leader/transfer/{next_leader}", post(to_member))
        .with_state(Leader { services, cancel })
}
