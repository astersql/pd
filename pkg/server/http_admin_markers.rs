// Copyright 2026 AsterSQL.
//! Durable recovery markers shared by HTTP and the classic PD RPC.
use crate::{PdServices, request_context};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::{Compare, Consistency, Mutation, Read};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(crate) async fn marker(
    services: &dyn PdServices,
    pitr: bool,
    change: Option<bool>,
) -> Result<bool, String> {
    let store = services
        .recovery_marker_store()
        .ok_or("server not started")?;
    let paths = ClusterPaths::new(services.identity().cluster_id);
    let key = if pitr {
        paths.pitr_restore_mode_mark_path()
    } else {
        paths.recovering_mark_path()
    }
    .into_bytes();
    match change {
        Some(true) => {
            store
                .transact(
                    vec![Compare::CreateRevision(key.clone(), 0)],
                    vec![Mutation::put(&key, b"on")],
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(true)
        }
        Some(false) => {
            store
                .transact(vec![], vec![Mutation::Delete { key }])
                .await
                .map_err(|e| e.to_string())?;
            Ok(false)
        }
        None => {
            let s = store
                .read(Read::range(&key, b"", Consistency::Linearizable))
                .await
                .map_err(|e| e.to_string())?;
            Ok(!s.entries.is_empty())
        }
    }
}
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route(
            "/pd/api/v1/admin/cluster/markers/snapshot-recovering",
            get(handle).post(handle).delete(handle),
        )
        .route(
            "/pd/api/v1/admin/cluster/markers/pitr-restore-mode",
            get(handle).post(handle).delete(handle),
        )
        .with_state(Context { services, cancel })
}
async fn handle(
    State(s): State<Context>,
    OriginalUri(uri): OriginalUri,
    method: Method,
) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let change = match method {
        Method::POST => Some(true),
        Method::DELETE => Some(false),
        _ => None,
    };
    let result = tokio::select! { biased;
        _ = cancel.cancelled() => Err("context canceled".into()),
        result = marker(s.services.as_ref(), uri.path().ends_with("pitr-restore-mode"), change) => result,
    };
    match result {
        Ok(marked) if change.is_none() => {
            // Go's render.JSON encodes a trailing newline for this response.
            let body = serde_json::to_string_pretty(&serde_json::json!({"marked": marked}))
                .expect("boolean marker JSON");
            (
                StatusCode::OK,
                [("content-type", "application/json; charset=UTF-8")],
                format!("{body}\n"),
            )
                .into_response()
        }
        Ok(_) => (StatusCode::OK, "").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}
