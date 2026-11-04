// Copyright 2026 AsterSQL.
//! Replication HTTP/protobuf/file boundaries over the host's one domain manager.
use crate::{PdServices, http_contract::json_response};
use astersql_pd_proto::replication_modepb::{DrAutoSync, ReplicationMode, ReplicationStatus};
pub use astersql_pd_replication::ModeSnapshot;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub fn to_proto(snapshot: &ModeSnapshot) -> ReplicationStatus {
    let dr = &snapshot.dr_auto_sync;
    let enabled = snapshot.config.replication_mode == "dr-auto-sync";
    ReplicationStatus {
        mode: if enabled {
            ReplicationMode::DrAutoSync as i32
        } else {
            ReplicationMode::Majority as i32
        },
        dr_auto_sync: enabled.then(|| DrAutoSync {
            label_key: snapshot.config.dr_auto_sync.label_key.clone(),
            state: match dr.state.as_str() {
                "async_wait" => 1,
                "async" => 2,
                "sync_recover" => 3,
                _ => 0,
            },
            state_id: dr.state_id,
            wait_sync_timeout_hint: 60,
            available_stores: dr.available_stores.clone(),
            pause_region_split: snapshot.config.dr_auto_sync.pause_region_split
                && dr.state != "sync",
        }),
    }
}
pub fn http_status(snapshot: &ModeSnapshot) -> serde_json::Value {
    use serde_json::json;
    let mut dr = json!({"label_key":"","state":"","acid_consistent":false});
    if snapshot.config.replication_mode == "dr-auto-sync" {
        let value = &snapshot.dr_auto_sync;
        dr["label_key"] = json!(snapshot.config.dr_auto_sync.label_key);
        dr["state"] = json!(value.state);
        dr["acid_consistent"] = json!(value.state != "sync_recover");
        if value.state_id != 0 {
            dr["state_id"] = json!(value.state_id);
        }
        if value.total_regions != 0 {
            dr["total_regions"] = json!(value.total_regions);
        }
        if value.synced_regions != 0 {
            dr["synced_regions"] = json!(value.synced_regions);
        }
        if value.recover_progress != 0.0 {
            dr["recover_progress"] = json!(value.recover_progress);
        }
    }
    json!({"mode":snapshot.config.replication_mode,"dr-auto-sync":dr})
}
#[derive(Clone)]
struct Host {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/replication_mode/status", get(status))
        .route("/pd/api/v1/admin/persist-file/{name}", post(persist))
        .with_state(Host { services, cancel })
}
async fn status(State(host): State<Host>) -> Response {
    let result = tokio::select! {biased;_=host.cancel.cancelled()=>Err("server not started".into()),r=host.services.replication_mode()=>r};
    match result {
        Ok(Some(state)) => json_response(StatusCode::OK, &http_status(&state)),
        Ok(None) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
        ),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}
async fn persist(State(host): State<Host>, Path(name): Path<String>, body: Body) -> Response {
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "").into_response(),
        };
        if !valid_file_json(&bytes) {
            return (StatusCode::BAD_REQUEST, "body should be json format").into_response();
        }
        if name != "DR_STATE" {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid file name").into_response();
        }
        match host.services.persist_dr_state(bytes.to_vec()).await {
            Ok(()) => (StatusCode::OK, "").into_response(),
            Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
        }
    };
    tokio::select! {biased;_=host.cancel.cancelled()=>(StatusCode::SERVICE_UNAVAILABLE,"server not started").into_response(),r=work=>r}
}
/// Replicate the current file to actual registered members, retrying only failures.
/// Cache and TLS/config ownership remain with the active host term.
pub async fn replicate(
    store: &dyn astersql_pd_storage::MetaStore,
    cluster: u64,
    snapshot: &ModeSnapshot,
    config: &astersql_pd_server_config::Config,
    cache: &mut BTreeMap<u64, u64>,
    cancel: CancellationToken,
) -> Result<(), String> {
    if snapshot.config.replication_mode != "dr-auto-sync" {
        return Ok(());
    }
    let dr = &snapshot.dr_auto_sync;
    let (members, _) = crate::grpc_members::registry_members(store, cluster)
        .await
        .map_err(|e| e.to_string())?;
    let mut file = serde_json::json!({});
    if !dr.state.is_empty() {
        file["state"] = serde_json::json!(dr.state);
    }
    if dr.state_id != 0 {
        file["state_id"] = serde_json::json!(dr.state_id);
    }
    if !dr.available_stores.is_empty() {
        file["available_stores"] = serde_json::json!(dr.available_stores);
    }
    if let Some(start) = dr.recover_start {
        file["recover_start"] = serde_json::json!(start);
    }
    let data = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
    let mut failure = None;
    for member in members {
        if cancel.is_cancelled() {
            return Err("operation cancelled".into());
        }
        if cache.get(&member.member_id) == Some(&dr.state_id) {
            continue;
        }
        let operation = async {
            let url = member.client_urls.first().ok_or("client URL is empty")?;
            let target = crate::metric_transport::Target::parse(&format!(
                "{}/pd/api/v1/admin/persist-file/DR_STATE",
                url.trim_end_matches('/')
            ))?;
            let request = Request::builder()
                .method("POST")
                .uri(target.uri.path_and_query().unwrap().as_str())
                .header("host", target.uri.authority().unwrap().as_str())
                .header("pd-allow-follower-handle", "true")
                .body(Body::from(data.clone()))
                .map_err(|e| e.to_string())?;
            let response = crate::metric_transport::send(
                target,
                request,
                config,
                false,
                None,
                cancel.clone(),
                tokio::time::Instant::now() + Duration::from_secs(5),
            )
            .await?;
            if response.status() != StatusCode::OK {
                return Err(format!("replication returned {}", response.status()));
            }
            Ok::<_, String>(())
        }
        .await;
        match operation {
            Ok(()) => {
                cache.insert(member.member_id, dr.state_id);
            }
            Err(e) => failure = Some(e),
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Gogo CompactTextString used by the existing cluster status response.
pub fn compact_status(snapshot: &ModeSnapshot) -> String {
    use std::fmt::Write;
    let proto = to_proto(snapshot);
    let Some(dr) = proto.dr_auto_sync else {
        return String::new();
    };
    let mut out = String::from("mode:DR_AUTO_SYNC dr_auto_sync:<");
    if !dr.label_key.is_empty() {
        write!(
            out,
            "label_key:{} ",
            crate::grpc_ask_split::quote_bytes(dr.label_key.as_bytes())
        )
        .unwrap();
    }
    if dr.state != 0 {
        write!(
            out,
            "state:{} ",
            match dr.state {
                1 => "ASYNC_WAIT",
                2 => "ASYNC",
                3 => "SYNC_RECOVER",
                _ => unreachable!(),
            }
        )
        .unwrap();
    }
    if dr.state_id != 0 {
        write!(out, "state_id:{} ", dr.state_id).unwrap();
    }
    write!(out, "wait_sync_timeout_hint:{} ", dr.wait_sync_timeout_hint).unwrap();
    for id in dr.available_stores {
        write!(out, "available_stores:{id} ").unwrap();
    }
    if dr.pause_region_split {
        out.push_str("pause_region_split:true ");
    }
    out.push_str("> ");
    out
}

// json.Valid checks syntax, not representability as f64 or a typed model.
fn valid_file_json(bytes: &[u8]) -> bool {
    let normalized = crate::http_contract::go_json_strings(bytes);
    let mut depth = 0_i32;
    let mut quoted = false;
    let mut escaped = false;
    for &byte in &normalized {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > 10_000 {
                        return false;
                    }
                }
                b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
    }
    serde_json::from_slice::<serde::de::IgnoredAny>(&normalized).is_ok()
}
