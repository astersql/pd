// Copyright 2026 AsterSQL.
//! Scheduler-local grant HTTP adapter over canonical transactions.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_core::{BasicCluster, StoreControlError};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{GrantConfigError, GrantLeaderScheduler, GrantRemoveError};
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
fn config_error(e: GrantConfigError) -> String {
    match e {
        GrantConfigError::Pause(StoreControlError::StoreNotFound(id)) => {
            format!("[PD:core:ErrStoreNotFound]store {id} not found")
        }
        _ => e.to_string(),
    }
}
/// Dispatch relative `/config`, `/list`, and `/delete/{store_id}` routes. The host
/// owns mounting, synchronization and scheduler removal; exclusive access spans I/O.
pub async fn serve(
    scheduler: &mut GrantLeaderScheduler,
    cluster: &BasicCluster,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
    remove: impl FnOnce(&str) -> Result<(), GrantRemoveError>,
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
            .delete_grant_store(id, cluster, storage, paths, remove)
            .await
        {
            Ok(result) => json_response(
                200,
                result
                    .map(|s| Value::String(s.into()))
                    .unwrap_or(Value::Null),
            ),
            Err(GrantConfigError::MissingStore(_)) => message(
                404,
                "[PD:scheduler:ErrScheduleConfigNotExist]the config does not exist",
            ),
            Err(GrantConfigError::Remove(GrantRemoveError::NotFound)) => message(
                404,
                "[PD:scheduler:ErrSchedulerNotFound]scheduler not found",
            ),
            Err(e) => message(500, config_error(e)),
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
    let Some(id) = input["store_id"].as_f64() else {
        cluster
            .stores
            .resume_leader_transfer(0, astersql_pd_core_constant::Direction::OUT);
        return message(
            400,
            "[PD:scheduler:ErrSchedulerConfig]wrong scheduler config id",
        );
    };
    // encoding/json decodes ranges as []interface{}, never []string: ignore it.
    let retained: Vec<Vec<u8>> = scheduler
        .config()
        .stores()
        .get(&(id as u64))
        .into_iter()
        .flatten()
        .flat_map(|r| [r.start_key.clone(), r.end_key.clone()])
        .collect();
    match scheduler
        .update_grant_config(id as u64, None, cluster, storage, paths)
        .await
    {
        Ok(()) => message(200, "The scheduler has been applied to the store."),
        Err(GrantConfigError::Range(e)) => {
            let bytes = &retained[e.argument];
            // The canonical API percent-encodes non-ASCII bytes before decoding;
            // its offset is in that expanded input. Locate the bad escape in the
            // original bytes rather than indexing them with the expanded offset.
            let offset = bytes
                .iter()
                .enumerate()
                .find_map(|(i, b)| {
                    (*b == b'%'
                        && !bytes
                            .get(i + 1..i + 3)
                            .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit)))
                    .then_some(i)
                })
                .expect("range decoding failed on an invalid percent escape");
            let escape = String::from_utf8_lossy(&bytes[offset..(offset + 3).min(bytes.len())]);
            message(
                400,
                format!(
                    "[PD:url:ErrQueryUnescape]inverse transformation of QueryEscape error: invalid URL escape {}",
                    serde_json::to_string(&escape).unwrap()
                ),
            )
        }
        Err(e) => message(500, config_error(e)),
    }
}
