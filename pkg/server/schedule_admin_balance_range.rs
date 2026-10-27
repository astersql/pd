// Copyright 2026 AsterSQL.
//! Balance-range scheduler's local configuration and job HTTP boundary.
use crate::schedule_admin_balance_leader::{json_type, syntax_error};
use astersql_pd_schedule_schedulers::{BalanceRangeJob, BalanceRangeJobs, JobTime, get_key_ranges};
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
fn encoded<T: serde::Serialize + ?Sized>(v: &T) -> String {
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
        .body(Body::from(format!("{body}\n")))
        .unwrap()
}
fn json_response(status: u16, body: Value) -> Response {
    response(status, encoded(&body))
}
fn message(status: u16, text: impl AsRef<str>) -> Response {
    response(status, encoded(text.as_ref()))
}
/// Serve scheduler-relative paths. The host owns the clock, mounting, locking,
/// and HEAD wire-body suppression. Exclusive borrowing spans persistence.
pub async fn serve(jobs: &mut BalanceRangeJobs, now: JobTime, request: Request<Body>) -> Response {
    let path = request.uri().path();
    let allowed = match path {
        "/config" => request.method() == Method::POST,
        "/list" => request.method() == Method::GET,
        "/job" => request.method() == Method::PUT || request.method() == Method::DELETE,
        _ => {
            return Response::builder()
                .status(404)
                .header("content-type", "text/plain; charset=utf-8")
                .header("x-content-type-options", "nosniff")
                .body(Body::from("404 page not found\n"))
                .unwrap();
        }
    };
    if !allowed {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if path == "/config" {
        return message(400, "update config is not supported");
    }
    if path == "/list" {
        return response(200, encoded(jobs.jobs()));
    }
    if request.method() == Method::DELETE {
        // QueryUnescape is byte-oriented, keeps the first value, and ignores malformed
        // pairs (including unescaped semicolons), just like URL.Query().Get in Go.
        let id = request
            .uri()
            .query()
            .unwrap_or("")
            .split('&')
            .filter(|p| !p.contains(';'))
            .filter_map(|p| {
                let (key, val) = p.split_once('=').unwrap_or((p, ""));
                let pair = get_key_ranges(&[key, val]).ok()?.remove(0);
                (pair.start_key == b"job-id").then_some(pair.end_key)
            })
            .next()
            .unwrap_or_default();
        if id.is_empty() {
            return message(400, "job-id is required");
        }
        let parsed = std::str::from_utf8(&id)
            .ok()
            .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse::<u64>().ok());
        let Some(id) = parsed else {
            return message(400, "invalid job-id");
        };
        return match jobs.delete(id, now).await {
            Ok(()) => response(200, "null".into()),
            // FastGenByArgs returns a withStack wrapper even for typed
            // missing/completed errors. JSON sees no exported fields on it,
            // just as for a wrapped I/O failure: preserve {} rather than
            // exposing the inner error's MarshalJSON or Error string.
            Err(_) => response(500, "{}".into()),
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

    let input = input.as_object().unwrap();
    // Match Go's validation order and HTTP diagnostics before invoking the
    // canonical domain decoder; queue mutation/persistence remain domain-owned.
    for field in ["engine", "rule", "alias"] {
        let Some(value) = input
            .get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return message(400, format!("{field} is required and must be a string"));
        };
        if field == "engine" && !matches!(value, "tikv" | "tiflash") {
            return message(400, format!("engine:{value} must be tikv or tiflash"));
        }
        if field == "rule"
            && !matches!(value, "leader-scatter" | "learner-scatter" | "peer-scatter")
        {
            return message(
                400,
                format!("rule:{value} must be leader-scatter, learner-scatter or peer-scatter"),
            );
        }
    }
    if let Some(timeout) = input.get("timeout") {
        let Some(text) = timeout.as_str() else {
            return message(400, "timeout must be a string");
        };
        if !text.is_empty() {
            match text.parse::<astersql_pd_typeutil::Duration>() {
                Err(_) => return message(400, format!("timeout:{text} is invalid")),
                Ok(d) if d.as_nanos() <= 0 => return message(400, "timeout must be positive"),
                Ok(_) => {}
            }
        }
    }
    for field in ["start-key", "end-key"] {
        if !input.get(field).is_some_and(Value::is_string) {
            return message(
                400,
                format!("[PD:common:ErrInvalidArgument]invalid argument for {field}: %!v(MISSING)"),
            );
        }
    }
    let start = input["start-key"].as_str().unwrap();
    let end = input["end-key"].as_str().unwrap();
    if start.split(',').count() != end.split(',').count() {
        return message(
            400,
            format!("[PD:common:ErrInvalidArgument]invalid argument for {start}: {end}"),
        );
    }
    let keys = match astersql_pd_keyutil::decode_http_key_ranges(input) {
        Ok(keys) => keys,
        Err(e) => return message(400, e.to_string()),
    };
    // The Go handler decodes URL escapes twice. Check the second pass here to
    // retain its typed wire error; from_http constructs the actual job/ranges.
    for key in &keys {
        if let Some(offset) = key.iter().enumerate().find_map(|(i, b)| {
            (*b == b'%'
                && !key
                    .get(i + 1..i + 3)
                    .is_some_and(|p| p.iter().all(u8::is_ascii_hexdigit)))
            .then_some(i)
        }) {
            let escape = String::from_utf8_lossy(&key[offset..(offset + 3).min(key.len())]);
            return message(
                400,
                format!(
                    "[PD:url:ErrQueryUnescape]inverse transformation of QueryEscape error: invalid URL escape {}",
                    serde_json::to_string(&escape).unwrap()
                ),
            );
        }
    }
    let job = match BalanceRangeJob::from_http(input, now) {
        Ok(job) => job,
        Err(e) => return message(400, e.to_string()),
    };
    match jobs.add(job).await {
        Ok(_) => response(200, "null".into()),
        Err(e) => message(400, e.to_string()),
    }
}
