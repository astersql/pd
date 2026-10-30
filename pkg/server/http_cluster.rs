// Copyright 2026 AsterSQL.
//! Cluster inspection from canonical host state and persisted bootstrap metadata.
use crate::{PdServices, http_contract::json_response, request_context};
use astersql_pd_storage::{Consistency, Read};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::Response,
    routing::{any, get},
};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Host-owned cluster configuration and initialization observation.
pub struct ClusterObservation {
    pub max_peer_count: u32,
    pub initialized: bool,
    /// None before the bootstrap replication manager exists.
    pub replication_status: Option<String>,
}
#[derive(Clone)]
struct Cluster {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
    start_timestamp: i64,
}
impl Cluster {
    fn token(&self) -> CancellationToken {
        request_context::current()
            .map(|c| c.cancel)
            .unwrap_or_else(|| self.cancel.child_token())
    }
}
async fn inspect(s: Cluster, status: bool) -> Response {
    let cancel = s.token();
    let work = async {
        let observation = s.services.cluster_observation().await?;
        if !status {
            let mut body = json!({});
            if s.services.identity().cluster_id != 0 {
                body["id"] = json!(s.services.identity().cluster_id);
            }
            if observation.max_peer_count != 0 {
                body["max_peer_count"] = json!(observation.max_peer_count);
            }
            return Ok(body);
        }
        let store = s.services.discovery_store().ok_or("server not started")?;
        let paths = astersql_pd_keypath::ClusterPaths::new(s.services.identity().cluster_id);
        let snapshot = store
            .read(Read::range(
                paths.cluster_bootstrap_time_path().as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .map_err(|e| e.to_string())?;
        let bytes = snapshot
            .entries
            .first()
            .map(|e| e.value.as_slice())
            .unwrap_or_default();
        let time = if bytes.is_empty() {
            "0001-01-01T00:00:00Z".into()
        } else {
            let time = astersql_pd_typeutil::parse_timestamp(bytes)
                .map_err(|e| format!("[PD:typeutil:ErrBytesToUint64]{e}"))?;
            let time: chrono::DateTime<chrono::Utc> = time.into();
            // Go RFC3339Nano removes trailing fractional zeroes.
            let mut text = time.format("%Y-%m-%dT%H:%M:%S%.9f").to_string();
            while text.ends_with('0') {
                text.pop();
            }
            if text.ends_with('.') {
                text.pop();
            }
            text.push('Z');
            text
        };
        Ok::<_, String>(
            json!({"raft_bootstrap_time":time,"is_initialized":!bytes.is_empty() && observation.initialized,"replication_status":observation.replication_status.unwrap_or_default()}),
        )
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"),
        result = work => match result {
            Ok(body) => json_response(StatusCode::OK, &body),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    }
}
async fn cluster(State(s): State<Cluster>) -> Response {
    inspect(s, false).await
}
async fn cluster_status(State(s): State<Cluster>) -> Response {
    inspect(s, true).await
}
const VERSION: &str = match option_env!("PD_RELEASE_VERSION") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};
async fn version() -> Response {
    json_response(
        StatusCode::OK,
        &json!({"version":VERSION,"build_time":"","hash":"","branch":""}),
    )
}
async fn status(State(s): State<Cluster>) -> Response {
    json_response(
        StatusCode::OK,
        &json!({"build_ts":option_env!("PD_BUILD_TS").unwrap_or("None"),"version":VERSION,"git_hash":option_env!("PD_GIT_HASH").unwrap_or("None"),"start_timestamp":s.start_timestamp,"kernel_type":"Classic"}),
    )
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    let start_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Router::new()
        .route("/pd/api/v1/cluster", get(cluster))
        .route("/pd/api/v1/cluster/status", any(cluster_status))
        .route("/pd/api/v1/status", get(status))
        .route("/pd/api/v1/version", get(version))
        .with_state(Cluster {
            services,
            cancel,
            start_timestamp,
        })
}
