// Copyright 2026 AsterSQL.
//! Maintenance HTTP adapter over the shared, transactional storage endpoint.
use crate::PdServices;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::{MAINTENANCE_LOCK_NAME, MaintenanceTask, StorageEndpoint};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/maintenance", get(all))
        .route("/pd/api/v2/maintenance/{task_type}", get(one))
        .route(
            "/pd/api/v2/maintenance/{task_type}/{task_id}",
            axum::routing::put(change).delete(change),
        )
        .with_state(Context { services, cancel })
}
fn response(status: u16, value: Value) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        axum::Json(value),
    )
        .into_response()
}
impl Context {
    fn endpoint(&self) -> Result<(StorageEndpoint, ClusterPaths), String> {
        Ok((
            StorageEndpoint::new(
                self.services
                    .recovery_marker_store()
                    .ok_or("server not started")?,
            ),
            ClusterPaths::new(self.services.identity().cluster_id),
        ))
    }
    async fn run(
        &self,
        f: impl std::future::Future<Output = Result<(u16, Value), String>>,
    ) -> Response {
        let cancel = crate::request_context::current()
            .map(|c| c.cancel)
            .unwrap_or_else(|| self.cancel.child_token());
        let result =
            tokio::select! {biased; _=cancel.cancelled()=>Err("context canceled".into()), r=f=>r};
        match result {
            Ok((status, value)) => response(status, value),
            Err(error) => response(500, json!(error)),
        }
    }
}
async fn all(State(s): State<Context>) -> Response {
    s.run(async {
        let (endpoint, paths) = s.endpoint()?;
        let mut tasks = Vec::new();
        endpoint
            .load_all_maintenance_tasks(&paths, |key, value| {
                if key == MAINTENANCE_LOCK_NAME.as_bytes() {
                    return;
                }
                if let Ok(task) =
                    serde_json::from_str::<MaintenanceTask>(&String::from_utf8_lossy(value))
                {
                    tasks.push(task);
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(if tasks.is_empty() {
            (404, json!("No maintenance task is running"))
        } else {
            (200, json!(tasks))
        })
    })
    .await
}
async fn one(State(s): State<Context>, Path(task_type): Path<String>) -> Response {
    s.run(async {
        let (endpoint, paths) = s.endpoint()?;
        let task = endpoint
            .load_maintenance_task(&paths, &task_type)
            .await
            .map_err(|e| e.to_string())?;
        Ok(match task {
            Some(task) => (200, json!(task)),
            None => (404, json!("No maintenance task is running for this type")),
        })
    })
    .await
}
async fn change(
    State(s): State<Context>,
    Path((task_type, task_id)): Path<(String, String)>,
    method: Method,
    body: Body,
) -> Response {
    s.run(async {
        let (endpoint, paths) = s.endpoint()?;
        if method == Method::PUT {
            let bytes = to_bytes(body, usize::MAX).await.map_err(|e| e.to_string())?;
            let start_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs() as i64;
            let task = MaintenanceTask {
                task_type,
                id: task_id,
                start_timestamp,
                description: String::from_utf8_lossy(&bytes).into_owned(),
            };
            let (ok, existing) = endpoint
                .try_start_maintenance_task_atomic(&paths, &task)
                .await
                .map_err(|e| e.to_string())?;
            if ok {
                if let Some(metrics) = s.services.service_metrics() {
                    metrics.maintenance_tasks.with_label_values(&[&task.task_type, &task.id]).set(1.0);
                }
                Ok((200, json!("Maintenance task started successfully.")))
            } else {
                Ok((409, json!({"error": "Another maintenance task is already running", "existing_task": existing})))
            }
        } else {
            let (ok, existing) = endpoint
                .try_delete_maintenance_task_atomic(&paths, &task_type, &task_id)
                .await
                .map_err(|e| e.to_string())?;
            if ok {
                if let Some(metrics) = s.services.service_metrics() {
                    metrics.maintenance_tasks.with_label_values(&[&task_type, &task_id]).set(0.0);
                }
                Ok((200, json!("Maintenance task deleted successfully.")))
            } else if let Some(task) = existing {
                Ok((409, json!({"error": "Task ID does not match the current task", "existing_task": task})))
            } else {
                Ok((404, json!("No maintenance task is running for this type")))
            }
        }
    }).await
}
