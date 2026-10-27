// Copyright 2026 AsterSQL.
//! Scheduler-local eviction HTTP adapter over canonical transactions.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_core::{BasicCluster, StoreControlError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{EvictConfigError, EvictLeaderScheduler, EvictRemoveError};
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
fn go_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "<nil>",
        Value::Bool(_) => "bool",
        Value::String(_) => "string",
        Value::Number(_) => "float64",
        Value::Array(_) => "[]interface {}",
        Value::Object(_) => "map[string]interface {}",
    }
}
fn config_error(e: EvictConfigError, input: &Value) -> String {
    match e {
        EvictConfigError::Pause(StoreControlError::StoreNotFound(id)) => {
            format!("[PD:core:ErrStoreNotFound]store {id} not found")
        }
        EvictConfigError::Range(e) => {
            let s = input["ranges"][e.argument].as_str().unwrap();
            let b = s.as_bytes();
            let end = (e.offset + 3).min(b.len());
            format!(
                "[PD:url:ErrQueryUnescape]inverse transformation of QueryEscape error: invalid URL escape {}",
                serde_json::to_string(&String::from_utf8_lossy(&b[e.offset..end])).unwrap()
            )
        }
        _ => e.to_string(),
    }
}
/// Dispatch relative `/config`, `/list`, and `/delete/{store_id}` routes. The host
/// owns mounting, synchronization and scheduler removal; exclusive access spans I/O.
pub async fn serve(
    scheduler: &mut EvictLeaderScheduler,
    cluster: &BasicCluster,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
    remove: impl FnOnce(&str) -> Result<(), EvictRemoveError>,
) -> Response {
    let path = request.uri().path();
    let id = path
        .strip_prefix("/delete/")
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .map(str::to_owned);
    let method = match path {
        "/config" => Method::POST,
        "/list" => Method::GET,
        _ if id.is_some() => Method::DELETE,
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
        let mut snapshot = serde_json::to_value(scheduler.config()).unwrap();
        for ranges in snapshot["store-id-ranges"]
            .as_object_mut()
            .unwrap()
            .values_mut()
        {
            if ranges.as_array().is_some_and(Vec::is_empty) {
                *ranges = Value::Null;
            }
        }
        return json_response(200, snapshot);
    }
    if let Some(id) = id {
        let parsed = if !id.bytes().all(|c| c.is_ascii_digit()) {
            Err("invalid syntax")
        } else {
            id.parse::<u64>().map_err(|_| "value out of range")
        };
        let id = match parsed {
            Ok(id) => id,
            Err(e) => {
                return message(
                    400,
                    format!(
                        "strconv.ParseUint: parsing {}: {e}",
                        serde_json::to_string(&id).unwrap()
                    ),
                );
            }
        };
        return match scheduler
            .delete_evict_store(id, cluster, storage, paths, remove)
            .await
        {
            Ok(result) => json_response(
                200,
                result
                    .map(|s| Value::String(s.into()))
                    .unwrap_or(Value::Null),
            ),
            Err(EvictConfigError::MissingStore(_)) => message(
                404,
                "[PD:scheduler:ErrScheduleConfigNotExist]the config does not exist",
            ),
            Err(EvictConfigError::Remove(EvictRemoveError::NotFound)) => message(
                404,
                "[PD:scheduler:ErrSchedulerNotFound]scheduler not found",
            ),
            Err(e) => message(500, config_error(e, &Value::Null)),
        };
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
    // Validate IDs first, then batch, then ranges, matching Go's error precedence.
    // The canonical API validates again before any pause or persistence side effect.
    let fields = input.as_object().unwrap();
    if fields.contains_key("store_id") && fields.contains_key("store_ids") {
        return message(400, "only one of store_id and store_ids can be set");
    }
    let ids = if let Some(one) = fields.get("store_id") {
        vec![one]
    } else if let Some(many) = fields.get("store_ids") {
        match many.as_array().filter(|a| !a.is_empty()) {
            Some(a) => a.iter().collect(),
            None => return message(400, "please input a right store id"),
        }
    } else {
        vec![]
    };
    for v in &ids {
        if v.as_f64()
            .and_then(|f| astersql_pd_schedule_schedulers::evict_store_id_from_float(f).ok())
            .is_none()
        {
            return message(400, "please input a right store id");
        }
    }
    if let Some(batch) = fields.get("batch").filter(|v| !v.is_null()) {
        if !batch.is_number() {
            return message(
                400,
                format!(
                    "invalid argument for 'batch': expected a number, got {}",
                    go_type(batch)
                ),
            );
        }
        let f = batch.as_f64().unwrap();
        if !(1. ..=100.).contains(&f) || f.fract() != 0. {
            return message(400, "batch must be an integer in [1, 100]");
        }
    }
    if let Some(ranges) = fields.get("ranges") {
        if ids.is_empty() {
            return json_response(
                400,
                json!({"class":0,"code":0,"message":"wrong scheduler config id","rfccode":"PD:scheduler:ErrSchedulerConfig"}),
            );
        }
        if !ranges
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return message(
                400,
                format!(
                    "invalid argument for 'ranges': expected an array of strings, got {}",
                    go_type(ranges)
                ),
            );
        }
    }
    match scheduler
        .update_evict_config(&input, cluster, storage, paths)
        .await
    {
        Ok(()) => message(200, "The scheduler has been applied to the store."),
        Err(e) => message(400, config_error(e, &input)),
    }
}
