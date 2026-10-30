// Copyright 2026 AsterSQL.
//! Classic member administration; topology mutations are supplied by the host.
use crate::{PdServices, grpc_members, http_contract::*, request_context};
use astersql_pd_proto::pdpb;
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct MemberMetadata {
    pub binary_version: String,
    pub deploy_path: String,
    pub git_hash: String,
    pub leader_priority: i32,
}
#[derive(Clone)]
struct Members {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
fn token(s: &Members) -> CancellationToken {
    request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token())
}
fn failure(error: impl ToString) -> Response {
    json_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
}
fn member_json(m: &pdpb::Member) -> Value {
    let mut v = json!({"name":m.name,"member_id":m.member_id,"peer_urls":m.peer_urls,"client_urls":m.client_urls});
    for (key, text) in [
        ("binary_version", &m.binary_version),
        ("deploy_path", &m.deploy_path),
        ("git_hash", &m.git_hash),
    ] {
        if !text.is_empty() {
            v[key] = json!(text);
        }
    }
    if m.leader_priority != 0 {
        v["leader_priority"] = json!(m.leader_priority);
    }
    v
}
async fn list(s: &Members) -> Result<pdpb::GetMembersResponse, String> {
    let mut response = grpc_members::members(s.services.as_ref(), &token(s))
        .await
        .map_err(|e| e.message().to_owned())?;
    if let Some(error) = response.header.as_ref().and_then(|h| h.error.as_ref()) {
        return Err(error.message.clone());
    }
    for m in &mut response.members {
        let info = s.services.member_metadata(m.member_id).await;
        m.binary_version = info.binary_version;
        m.deploy_path = info.deploy_path;
        m.git_hash = info.git_hash;
        m.leader_priority = info.leader_priority;
    }
    Ok(response)
}
async fn members(State(s): State<Members>) -> Response {
    let cancel = token(&s);
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"),
        result = list(&s) => match result {
            Err(e) => failure(e),
            Ok(r) => {
                let mut body = json!({"header":{"cluster_id":s.services.identity().cluster_id},"members":r.members.iter().map(member_json).collect::<Vec<_>>()});
                if let Some(m) = r.leader { body["leader"] = member_json(&m); }
                if let Some(m) = r.etcd_leader { body["etcd_leader"] = member_json(&m); }
                json_response(StatusCode::OK, &body)
            }
        }
    }
}
async fn by_name(s: &Members, name: &str) -> Result<u64, Response> {
    let r = list(s).await.map_err(failure)?;
    r.members
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.member_id)
        .ok_or_else(|| json_response(StatusCode::NOT_FOUND, &format!("not found, pd: {name}")))
}
async fn set_property(State(s): State<Members>, Path(name): Path<String>, body: Body) -> Response {
    let cancel = token(&s);
    let work = async {
        let id = match by_name(&s, &name).await {
            Ok(id) => id,
            Err(r) => return r,
        };
        let input = match read_json_object(body).await {
            Ok(input) => input,
            Err(e) => return e.into_response(),
        };
        if let Some(value) = input.as_ref().and_then(|m| m.get("leader-priority")) {
            let Some(priority) = value.as_f64() else {
                return json_response(StatusCode::BAD_REQUEST, &"bad format leader priority");
            };
            // Go's amd64 float64->int conversion returns MinInt64 outside range.
            let priority = if !(-9223372036854775808.0..9223372036854775808.0).contains(&priority) {
                i64::MIN
            } else {
                priority as i64
            };
            if let Err(e) = s.services.set_member_priority(id, priority).await {
                return failure(e);
            }
        }
        json_response(StatusCode::OK, &"success")
    };
    tokio::select! { biased; _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"), result = work => result }
}
async fn remove(s: &Members, id: u64, label: &str) -> Response {
    match s.services.remove_member(id).await {
        Ok(()) => json_response(StatusCode::OK, &format!("removed, pd: {label}")),
        Err(e) => failure(e),
    }
}
async fn delete_name(State(s): State<Members>, Path(name): Path<String>) -> Response {
    let cancel = token(&s);
    let work = async {
        match by_name(&s, &name).await {
            Ok(id) => remove(&s, id, &name).await,
            Err(r) => r,
        }
    };
    tokio::select! { biased; _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"), result = work => result }
}
async fn delete_id(State(s): State<Members>, Path(id): Path<String>) -> Response {
    let parsed = match parse_uint64_field(Some(&id), "id") {
        Ok(id) => id,
        Err(HttpError::Input(e)) => return json_response(StatusCode::BAD_REQUEST, &e),
        Err(e) => return e.into_response(),
    };
    let cancel = token(&s);
    let label = parsed.to_string();
    tokio::select! { biased; _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"), result = remove(&s, parsed, &label) => result }
}
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/members", get(members))
        .route(
            "/pd/api/v1/members/name/{name}",
            delete(delete_name).post(set_property),
        )
        .route("/pd/api/v1/members/id/{id}", delete(delete_id))
        .with_state(Members { services, cancel })
}
