// Copyright 2026 AsterSQL.
//! Store lifecycle HTTP adaptation; mutations execute in the host coordinator.
use crate::{
    PdServices,
    http_contract::{json_response, parse_uint64_field, query_unescape},
    request_context,
};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, post},
};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub enum StoreMutation {
    Limit {
        id: Option<u64>,
        labels: Option<Vec<astersql_pd_core::StoreLabel>>,
        rate: f64,
        types: Vec<String>,
    },
    Offline {
        id: u64,
        force: bool,
    },
    Up(u64),
    RemoveTombstones,
    Labels {
        id: u64,
        labels: Vec<astersql_pd_core::StoreLabel>,
        force: bool,
    },
    DeleteLabel {
        id: u64,
        key: String,
    },
    Weight {
        id: u64,
        leader: f64,
        region: f64,
    },
}
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/store/{id}", delete(handle))
        .route("/pd/api/v1/store/{id}/state", post(handle))
        .route("/pd/api/v1/stores/remove-tombstone", delete(handle))
        .with_state(Context { services, cancel })
}
async fn handle(
    State(s): State<Context>,
    OriginalUri(uri): OriginalUri,
    method: Method,
) -> Response {
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        match s.services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        let q = uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter(|p| !p.contains(';'))
            .filter_map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                Some((
                    String::from_utf8_lossy(&query_unescape(k).ok()?).into_owned(),
                    String::from_utf8_lossy(&query_unescape(v).ok()?).into_owned(),
                ))
            })
            .collect::<Vec<_>>();
        let query = |key: &str| {
            q.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        };
        let (operation, message) = if uri.path().ends_with("/remove-tombstone") {
            (
                StoreMutation::RemoveTombstones,
                "Remove tombstone successfully.",
            )
        } else {
            let raw = uri
                .path()
                .trim_start_matches("/pd/api/v1/store/")
                .trim_end_matches("/state");
            let decoded = query_unescape(&raw.replace('+', "%2B"))
                .unwrap_or_else(|_| raw.as_bytes().to_vec());
            let text = String::from_utf8_lossy(&decoded);
            let id = match parse_uint64_field(Some(&text), "id") {
                Ok(id) => id,
                Err(crate::http_contract::HttpError::Input(msg)) => {
                    let mut r = json_response(
                        StatusCode::BAD_REQUEST,
                        &json!({"code":"input","msg":msg,"data":{}}),
                    );
                    r.headers_mut()
                        .insert("tidb-error-code", "input".parse().unwrap());
                    return r;
                }
                Err(e) => return e.into_response(),
            };
            if method == Method::DELETE {
                (
                    StoreMutation::Offline {
                        id,
                        force: q.iter().any(|(k, _)| k == "force"),
                    },
                    "The store is set as Offline.",
                )
            } else {
                let state = query("state");
                let op = if state.eq_ignore_ascii_case("up") {
                    StoreMutation::Up(id)
                } else if state.eq_ignore_ascii_case("offline") {
                    StoreMutation::Offline { id, force: false }
                } else {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        &format!("invalid state {state}"),
                    );
                };
                (op, "The store's state is updated.")
            }
        };
        let cleanup = matches!(operation, StoreMutation::RemoveTombstones);
        match s.services.mutate_store(operation).await {
            Ok(()) => json_response(StatusCode::OK, &message),
            Err(e) => {
                let status = if cleanup {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else if e.starts_with("[PD:core:ErrStoreNotFound]") {
                    StatusCode::NOT_FOUND
                } else if e.starts_with("[PD:core:ErrStoreRemoved]") {
                    StatusCode::GONE
                } else {
                    StatusCode::BAD_REQUEST
                };
                json_response(status, &e)
            }
        }
    };
    tokio::select! {biased;_=token.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),r=work=>r}
}
