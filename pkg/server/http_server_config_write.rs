// Copyright 2026 AsterSQL.
//! Top-level dynamic configuration writes on the shared listener.
use crate::{PdServices, http_contract::json_response, request_context};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::StatusCode,
    response::Response,
    routing::post,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Write {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/config", post(write))
        .route("/pd/api/v1/config/cluster-version", post(write))
        .with_state(Write { services, cancel })
}
async fn write(State(s): State<Write>, OriginalUri(uri): OriginalUri, body: Body) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let version = uri.path().ends_with("/cluster-version");
    let work = async {
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                let msg = crate::schedule_admin_balance_leader::syntax_error(&bytes, &e);
                if version {
                    return tagged(
                        StatusCode::BAD_REQUEST,
                        json!({"code":"input","msg":msg,"data":{"Offset":if e.is_eof(){bytes.len()}else{bytes.split_inclusive(|b|*b==b'\n').take(e.line().saturating_sub(1)).map(<[u8]>::len).sum::<usize>()+e.column()}}}),
                    );
                }
                return json_response(StatusCode::BAD_REQUEST, &msg);
            }
        };
        if !value.is_object() && !value.is_null() {
            let kind = crate::schedule_admin_balance_leader::json_type(&value);
            let typ = if version {
                "map[string]string"
            } else {
                "map[string]interface {}"
            };
            let msg = format!("json: cannot unmarshal {kind} into Go value of type {typ}");
            if version {
                let offset = if value.is_array() {
                    bytes
                        .iter()
                        .position(|b| !b.is_ascii_whitespace())
                        .unwrap_or(0)
                        + 1
                } else {
                    bytes
                        .iter()
                        .rposition(|b| !b.is_ascii_whitespace())
                        .map_or(0, |i| i + 1)
                };
                return tagged(
                    StatusCode::BAD_REQUEST,
                    json!({"code":"input","msg":msg,"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}}),
                );
            }
            return json_response(StatusCode::BAD_REQUEST, &msg);
        }
        if version {
            if let Some(fields) = value.as_object() {
                for (k, v) in fields {
                    if !v.is_string() && !v.is_null() {
                        let kind = crate::schedule_admin_balance_leader::json_type(v);
                        let (start, end) =
                            crate::http_schedule_config_write::field_bounds(&bytes, k)
                                .unwrap_or_default();
                        return tagged(
                            StatusCode::BAD_REQUEST,
                            json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go value of type string"),"data":{"Value":kind,"Type":{},"Offset":if v.is_object()||v.is_array(){start+1}else{end},"Struct":"","Field":""}}),
                        );
                    }
                }
            }
            let Some(v) = value.get("cluster-version") else {
                return tagged(
                    StatusCode::BAD_REQUEST,
                    json!({"code":"input","msg":"not set cluster-version","data":{}}),
                );
            };
            match s
                .services
                .patch_server_config(
                    "cluster-version".into(),
                    json!(v.as_str().unwrap_or_default()),
                )
                .await
            {
                Ok(()) => json_response(StatusCode::OK, &"The cluster version is updated."),
                Err(e) => tagged(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"code":"internal","msg":e,"data":{},"stack":[]}),
                ),
            }
        } else {
            // TTL scheduling requires its own lease-backed domain owner. Never
            // silently turn a temporary update into a persistent one.
            if uri.query().is_some_and(|q| {
                q.split('&')
                    .any(|v| v.starts_with("ttlSecond=") && v != "ttlSecond=")
            }) {
                return json_response(
                    StatusCode::NOT_IMPLEMENTED,
                    &"TTL configuration service is unavailable",
                );
            }
            match apply(s.services.as_ref(), value).await {
                Ok(()) => json_response(StatusCode::OK, &"The config is updated."),
                Err(e) => json_response(StatusCode::BAD_REQUEST, &e),
            }
        }
    };
    tokio::select! {biased; _=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"[PD:server:ErrServerNotStarted]server not started"),result=work=>result}
}
async fn apply(service: &dyn PdServices, mut patch: Value) -> Result<(), String> {
    normalize_numbers(&mut patch);
    let defaults = serde_json::to_value(astersql_pd_server_config::Config::default())
        .map_err(|e| e.to_string())?;
    let mut schedule = serde_json::Map::new();
    if let Some(fields) = patch.as_object() {
        for (key, value) in fields {
            let full = if key.contains('.') || defaults.get(key).is_some() {
                key.clone()
            } else {
                [
                    "schedule",
                    "replication",
                    "pd-server",
                    "replication-mode",
                    "keyspace",
                    "micro-service",
                    "controller",
                    "log",
                ]
                .iter()
                .find(|s| defaults[**s].get(key).is_some())
                .map(|s| format!("{s}.{key}"))
                .ok_or_else(|| format!("config item {key} not found"))?
            };
            let prefix = full.split('.').next().unwrap();
            let leaf = full.rsplit('.').next().unwrap();
            match prefix {
                "pd-server" | "cluster-version" => {
                    if prefix == "pd-server"
                        && matches!(
                            leaf,
                            "max-gap-reset-ts" | "min-resolved-ts-persistence-interval"
                        )
                    {
                        let text = value.as_str().ok_or("invalid syntax")?;
                        if serde_json::from_value::<astersql_pd_server_config::Config>(
                            json!({"pd-server":{leaf:text}}),
                        )
                        .is_err()
                        {
                            return Err(crate::http_schedule_config_write::duration_error(text));
                        }
                    }
                    service.patch_server_config(full, value.clone()).await?
                }
                "schedule" => {
                    if defaults["schedule"].get(leaf).is_none() {
                        return Err(format!("config item {leaf} not found"));
                    }
                    if schedule.insert(leaf.into(), value.clone()).is_some() {
                        return Err(format!("config item {leaf} specified more than once"));
                    }
                }
                "replication" => {
                    if defaults["replication"].get(leaf).is_none() {
                        return Err(format!("config item {leaf} not found"));
                    }
                    service
                        .patch_replication(false, json!({leaf:value}))
                        .await?;
                }
                "replication-mode" if full == prefix => {
                    return Err(format!("cannot update config prefix {prefix}"));
                }
                "replication-mode" | "keyspace" | "micro-service" | "controller" => {
                    return Err(format!("{prefix} configuration service is unavailable"));
                }
                _ => return Err(format!("config prefix {prefix} not found")),
            }
        }
    }
    if !schedule.is_empty() {
        service.patch_schedule(Value::Object(schedule)).await?;
    }
    Ok(())
}
fn tagged(status: StatusCode, value: Value) -> Response {
    let mut r = json_response(status, &value);
    r.headers_mut().insert(
        "tidb-error-code",
        value["code"].as_str().unwrap().parse().unwrap(),
    );
    r
}

// Go decodes top-level map values as float64, then marshals each typed patch.
fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for v in fields.values_mut() {
                normalize_numbers(v);
            }
        }
        Value::Array(values) => {
            for v in values {
                normalize_numbers(v);
            }
        }
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f < (i64::MAX as f64) {
                    *value = json!(f as i64);
                }
            }
        }
        _ => {}
    }
}
