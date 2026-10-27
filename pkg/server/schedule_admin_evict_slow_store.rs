// Copyright 2026 AsterSQL.
//! Scheduler-local HTTP configuration over canonical disk/network policy owners.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{EvictDiskScheduler, EvictNetworkScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
fn json_response(status: u16, body: Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=UTF-8")
        .body(Body::from(serde_json::to_string_pretty(&body).unwrap()))
        .unwrap()
}
fn message(status: u16, body: impl Into<String>) -> Response {
    json_response(status, Value::String(body.into()))
}
/// Dispatch `/config` and `/list`. The host serializes both policy owners and
/// mounts this relative handler; exclusive borrowing spans storage I/O.
pub async fn serve(
    disk: &mut EvictDiskScheduler,
    network: &mut EvictNetworkScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    disabled: bool,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path();
    let method = match path {
        "/config" => Method::POST,
        "/list" => Method::GET,
        _ => {
            return Response::builder()
                .status(404)
                .header("content-type", "text/plain; charset=utf-8")
                .body(Body::from("404 page not found\n"))
                .unwrap();
        }
    };
    if request.method() != method {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if path == "/list" {
        let mut snapshot = network.config().clone();
        snapshot.disk = disk.config().clone();
        let mut listed = serde_json::to_value(snapshot).unwrap();
        // Go listConfig constructs a new config with a zero-valued embedded base.
        listed["disabled"] = Value::Bool(false);
        return json_response(200, listed);
    }
    let mut bytes = Vec::new();
    let mut stream = request.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => bytes.extend_from_slice(&c),
            Err(e) => {
                let mut r = json_response(
                    500,
                    json!({"code":"internal","msg":e.to_string(),"data":{},"stack":std::backtrace::Backtrace::force_capture().to_string().lines().collect::<Vec<_>>()}),
                );
                r.headers_mut()
                    .insert("tidb-error-code", "internal".parse().unwrap());
                return r;
            }
        }
    }
    let mut input: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            let offset = if e.is_eof() {
                bytes.len()
            } else {
                bytes
                    .split(|b| *b == b'\n')
                    .take(e.line().saturating_sub(1))
                    .map(|s| s.len() + 1)
                    .sum::<usize>()
                    + e.column()
            };
            let mut r = json_response(
                400,
                json!({"code":"input","msg":syntax_error(&bytes,&e),"data":{"Offset":offset}}),
            );
            r.headers_mut()
                .insert("tidb-error-code", "input".parse().unwrap());
            return r;
        }
    };
    if input.is_null() {
        input = json!({})
    }
    if !input.is_object() {
        let offset = if input.is_array() {
            1
        } else {
            bytes
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0)
        };
        let mut r = json_response(
            400,
            json!({"code":"input","msg":format!("json: cannot unmarshal {} into Go value of type map[string]interface {{}}",json_type(&input)),"data":{"Value":json_type(&input),"Type":{},"Offset":offset,"Struct":"","Field":""}}),
        );
        r.headers_mut()
            .insert("tidb-error-code", "input".parse().unwrap());
        return r;
    }

    let recovery = input.get("recovery-duration").filter(|v| !v.is_null());
    if recovery.is_some_and(|v| !v.is_number()) {
        return message(400, "invalid argument for 'recovery-duration'");
    }
    let batch = input.get("batch").filter(|v| !v.is_null());
    if batch.is_some_and(|v| !v.is_number()) {
        return message(400, "invalid argument for 'batch'");
    }
    if batch.is_some_and(|v| {
        let n = v.as_f64().unwrap();
        !(1.0..=100.0).contains(&n) || n.fract() != 0.0
    }) {
        return message(400, "batch must be an integer in [1, 100]");
    }
    let enabled = input
        .get("enable-network-slow-store")
        .filter(|v| !v.is_null());
    if enabled.is_some_and(|v| !v.is_boolean()) {
        return message(400, "invalid argument for 'enable-network-slow-store'");
    }
    // Go converts float64 to uint64 after decoding. Nonnegative representable
    // values truncate; negative in-range values wrap through the signed conversion.
    let recovery = recovery
        .map(|v| {
            let n = v.as_f64().unwrap();
            if n < 0.0 { (n as i64) as u64 } else { n as u64 }
        })
        .unwrap_or(disk.config().recovery_sec());
    let batch = batch
        .map(|v| v.as_f64().unwrap() as usize)
        .unwrap_or(disk.config().batch());
    let enabled = enabled
        .map(|v| v.as_bool().unwrap())
        .unwrap_or(network.config().enabled);
    match network
        .update_with_disk(disk, enabled, recovery, batch, disabled, storage, paths)
        .await
    {
        Ok(()) => message(200, "Config updated."),
        Err(e) => message(500, e.to_string()),
    }
}
