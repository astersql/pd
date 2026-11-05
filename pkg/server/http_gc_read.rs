// Copyright 2026 AsterSQL.
//! HTTP observation delegates to the same term-owned GC manager as tonic.
use crate::{PdServices, http_contract::json_response};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/gc/safepoint/{id}", get(handle))
        .route("/pd/api/v1/gc/safepoint", get(legacy_list))
        .route(
            "/pd/api/v1/gc/safepoint/{name}",
            axum::routing::delete(legacy_delete),
        )
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    Path(raw): Path<String>,
) -> Response {
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        if !services.is_bootstrapped().await? {
            return Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!(
                    "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
                ),
            ));
        }
        let id = match raw.parse::<u32>() {
            Ok(id) if !raw.starts_with('-') && !raw.starts_with('+') => id,
            _ => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    serde_json::json!(format!("invalid keyspace ID: {raw}")),
                ));
            }
        };
        let state = services
            .gc_manager()
            .await?
            .get_gc_state_with_cancel(id, true, &cancel)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<_, String>((
            StatusCode::OK,
            serde_json::json!({"keyspace_id":id,"safe_point":state.gc_safe_point}),
        ))
    };
    let result =
        tokio::select! {biased;_=cancel.cancelled()=>Err("context canceled".into()),r=work=>r};
    match result {
        Ok((code, value)) => json_response(code, &value),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!(e)),
    }
}
async fn legacy_list(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
) -> Response {
    legacy(services, cancel, None).await
}
async fn legacy_delete(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    Path(name): Path<String>,
) -> Response {
    legacy(services, cancel, Some(name)).await
}
async fn legacy(
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
    name: Option<String>,
) -> Response {
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        let m = services.gc_manager().await?;
        if let Some(name) = name {
            m.force_delete_service_gc_safe_point(&name)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(serde_json::json!(
                "Delete service GC safepoint successfully."
            ));
        }
        let list = m
            .compatible_service_gc_safe_points(
                crate::grpc_gc_barrier::now().map_err(|e| e.to_string())?,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut value = serde_json::json!({"service_gc_safe_points":list.service_safe_points,"gc_safe_point":list.gc_safe_point});
        if list.minimum != 0 {
            value["min_service_gc_safe_point"] = serde_json::json!(list.minimum);
        }
        Ok::<_, String>(value)
    };
    match tokio::select! {biased;_=cancel.cancelled()=>Err("context canceled".into()),r=work=>r} {
        Ok(value) => json_response(StatusCode::OK, &value),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!(e)),
    }
}
