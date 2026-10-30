// Copyright 2026 AsterSQL.
//! Region cache administration using the host's canonical cluster and sync lifecycle.
use crate::{
    PdServices,
    http_contract::{HttpError, json_response, parse_uint64_field, query_unescape},
    request_context,
};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::delete,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
/// A cache reset, or removal of one region from cache and durable metadata.
#[derive(Clone, Copy)]
pub enum Command {
    Cache(Option<u64>),
    Storage(u64),
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/admin/cache/region/{id}", delete(handle))
        .route("/pd/api/v1/admin/cache/regions", delete(handle))
        .route("/pd/api/v1/admin/storage/region/{id}", delete(handle))
        .with_state(Context { services, cancel })
}
async fn handle(
    State(s): State<Context>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let storage = uri.path().contains("/storage/");
        let follower = s.services.healthy().await.is_err_and(|e| e == "not leader");
        let allowed = headers
            .get("pd-allow-follower-handle")
            .is_some_and(|v| !v.is_empty());
        if (!follower || storage || !allowed)
            && !s.services.is_bootstrapped().await.unwrap_or(false)
        {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            );
        }
        let command = if uri.path().ends_with("/cache/regions") {
            Command::Cache(None)
        } else {
            let raw = uri.path().rsplit('/').next().unwrap_or_default();
            let raw = query_unescape(&raw.replace('+', "%2B")).unwrap_or_default();
            let raw = String::from_utf8_lossy(&raw);
            let id = match parse_uint64_field(Some(&raw), "id") {
                Ok(id) => id,
                Err(HttpError::Input(e)) => return json_response(StatusCode::BAD_REQUEST, &e),
                Err(_) => unreachable!(),
            };
            if storage {
                Command::Storage(id)
            } else {
                Command::Cache(Some(id))
            }
        };
        match s
            .services
            .admin_region(command, follower && allowed && !storage)
            .await
        {
            Ok((code, value)) => {
                json_response(StatusCode::from_u16(code).expect("HTTP status"), &value)
            }
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased; _=token.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"), r=work=>r}
}
