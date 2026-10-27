// Copyright 2026 AsterSQL.
//! Scheduler-local hot-grant HTTP configuration over the canonical owner.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::GrantHotScheduler;
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
/// Dispatch relative `/config` and `/list`; the host serializes the scheduler
/// and mounts this handler. Exclusive borrowing spans storage I/O.
pub async fn serve(
    scheduler: &mut GrantHotScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
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
        let listed = serde_json::to_value(scheduler.config()).unwrap();
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

    // Preserve Go's validation order: store type, each store parse, leader type,
    // leader parse. Domain update_config validates the complete input again.
    fn config_error() -> Response {
        json_response(
            400,
            json!({"class":0,"code":0,"message":"wrong scheduler config %s","rfccode":"PD:scheduler:ErrSchedulerConfig"}),
        )
    }
    fn id_error() -> Response {
        json_response(
            400,
            json!({"class":0,"code":0,"message":"invalid data, must 8 bytes, but %d","rfccode":"PD:typeutil:ErrBytesToUint64"}),
        )
    }
    fn valid_id(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u64>().is_ok()
    }
    let Some(stores) = input["store-id"].as_str() else {
        return config_error();
    };
    if !stores.split(',').all(valid_id) {
        return id_error();
    }
    let Some(leader) = input["store-leader-id"].as_str() else {
        return config_error();
    };
    if !valid_id(leader) {
        return id_error();
    }
    match scheduler.update_config(&input, storage, paths).await {
        Ok(()) => json_response(200, Value::Null),
        Err(e) => message(500, e.to_string()),
    }
}
