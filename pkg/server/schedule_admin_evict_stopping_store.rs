// Copyright 2026 AsterSQL.
//! Scheduler-local stopping-store HTTP configuration over the canonical owner.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::EvictStoppingScheduler;
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
    scheduler: &mut EvictStoppingScheduler,
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
        let listed = json!({"batch":scheduler.config().batch(),
            "evicted-stores":scheduler.config().evicted_stores(),"disabled":false});
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

    let batch = input.get("batch").filter(|v| !v.is_null());
    if batch.is_some_and(|v| !v.is_number()) {
        return message(400, "invalid argument for 'batch'");
    }
    if batch.is_some_and(|v| !(1.0..=10.0).contains(&v.as_f64().unwrap())) {
        return message(400, "batch is invalid, it should be in [1, 10]");
    }
    match scheduler
        .update_http(batch.map(|v| v.as_f64().unwrap()), disabled, storage, paths)
        .await
    {
        Ok(()) => message(200, "Config updated."),
        Err(e) => message(500, e.to_string()),
    }
}
