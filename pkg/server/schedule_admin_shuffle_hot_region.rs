// Copyright 2026 AsterSQL.
//! Scheduler-local shuffle hot region configuration transport.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{ShuffleHotConfig, ShuffleHotScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
fn encoded<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v)
        .unwrap()
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}
fn response(status: u16, body: String) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=UTF-8")
        .body(Body::from(body))
        .unwrap()
}
fn json_response(status: u16, body: Value) -> Response {
    response(status, encoded(&body))
}
/// Dispatch relative paths. The host owns mounting and synchronization; exclusive
/// borrowing spans publication and I/O. Failed persistence preserves the old limit.
pub async fn serve(
    scheduler: &mut ShuffleHotScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
) -> Response {
    let path = request.uri().path();
    let method = match path {
        "/list" => Method::GET,
        "/config" => Method::POST,
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
        return response(200, encoded(scheduler.config()));
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
    let Some(limit) = input["limit"].as_f64() else {
        return json_response(400, json!("invalid limit"));
    };
    // Go JSON decodes numbers as float64 before truncating to uint64.
    // Signed negative values wrap; out-of-range conversions are platform-specific.
    let limit = if limit < 0.0 {
        (limit as i64) as u64
    } else {
        limit as u64
    };
    match scheduler
        .update(ShuffleHotConfig { limit }, storage, paths)
        .await
    {
        Ok(()) => response(200, "null".into()),
        Err(error) => response(500, encoded(&error.to_string())),
    }
}
