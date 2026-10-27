// Copyright 2026 AsterSQL.
//! Scheduler-local roles HTTP boundary; the host owns mounting and synchronization.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::{ShuffleRegionError, ShuffleRegionScheduler};
use astersql_pd_storage_endpoint::StorageEndpoint;
use axum::{
    body::Body,
    http::{Method, Request},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
fn response(status: u16, body: String, json: bool) -> Response {
    let mut result = Response::builder()
        .status(status)
        .header(
            "content-type",
            if json {
                "application/json; charset=UTF-8"
            } else {
                if status == 404 {
                    "text/plain; charset=utf-8"
                } else {
                    "text/plain; charset=UTF-8"
                }
            },
        )
        .body(Body::from(body))
        .unwrap();
    if status == 404 {
        result
            .headers_mut()
            .insert("x-content-type-options", "nosniff".parse().unwrap());
    }
    result
}
fn json_response(status: u16, value: Value) -> Response {
    // encoding/json's default HTML escaping is observable in role strings/errors.
    let text = serde_json::to_string_pretty(&value)
        .unwrap()
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    response(status, text + "\n", true)
}
fn input_error(msg: String, data: Value) -> Response {
    let mut r = json_response(400, json!({"code":"input","msg":msg,"data":data}));
    r.headers_mut()
        .insert("tidb-error-code", "input".parse().unwrap());
    r
}
fn type_error(value: &Value, offset: usize, typ: &str) -> Response {
    let kind = crate::schedule_admin_balance_leader::json_type(value);
    input_error(
        format!("json: cannot unmarshal {kind} into Go value of type {typ}"),
        json!({"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}),
    )
}
/// Dispatch relative GET `/list`, GET `/roles` and POST `/roles` as the Go
/// configuration handler does. Exclusive borrowing spans validation and persistence.
/// No global server/API route or private runtime is installed.
pub async fn serve(
    scheduler: &mut ShuffleRegionScheduler,
    storage: &StorageEndpoint,
    paths: &ClusterPaths,
    request: Request<Body>,
) -> Response {
    // Gorilla routes on decoded URL.Path and cleans before method dispatch.
    let encoded = request.uri().path().as_bytes();
    let mut decoded = vec![];
    let mut i = 0;
    while i < encoded.len() {
        if encoded[i] == b'%'
            && i + 2 < encoded.len()
            && let Ok(hex) = std::str::from_utf8(&encoded[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            decoded.push(byte);
            i += 3;
            continue;
        }
        decoded.push(encoded[i]);
        i += 1;
    }
    let path = String::from_utf8_lossy(&decoded);
    let mut parts = vec![];
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut clean = format!("/{}", parts.join("/"));
    if path.ends_with('/') && clean != "/" {
        clean.push('/');
    }
    if clean != path {
        let mut location = String::new();
        if let Some(scheme) = request.uri().scheme_str() {
            location.push_str(scheme);
            location.push_str("://");
            location.push_str(
                request
                    .uri()
                    .authority()
                    .map(|a| a.as_str())
                    .unwrap_or_default(),
            );
        }
        for byte in clean.bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&byte) {
                location.push(byte as char);
            } else {
                location.push_str(&format!("%{byte:02X}"));
            }
        }
        if let Some(query) = request.uri().query() {
            location.push('?');
            location.push_str(query);
        }
        return Response::builder()
            .status(301)
            .header("location", location)
            .body(Body::empty())
            .unwrap();
    }
    let path = path.as_ref();
    if !matches!(path, "/list" | "/roles") {
        return response(404, "404 page not found\n".into(), false);
    }
    if request.method() != Method::GET && !(path == "/roles" && request.method() == Method::POST) {
        return Response::builder().status(405).body(Body::empty()).unwrap();
    }
    if request.method() == Method::GET {
        return json_response(
            200,
            serde_json::to_value(scheduler.config().roles()).unwrap(),
        );
    }
    let mut bytes = vec![];
    let mut stream = request.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
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
    let value: Value = match serde_json::from_slice(&bytes) {
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
            let msg = if e.is_eof() {
                "unexpected end of JSON input".into()
            } else if e.to_string().starts_with("trailing characters") {
                format!(
                    "invalid character '{}' after top-level value",
                    bytes.get(offset.saturating_sub(1)).copied().unwrap_or(b'?') as char
                )
            } else {
                crate::schedule_admin_balance_leader::syntax_error(&bytes, &e)
            };
            return input_error(msg, json!({"Offset":offset}));
        }
    };
    let roles = if value.is_null() {
        None
    } else if let Some(array) = value.as_array() {
        let mut roles = vec![];
        let mut offset = bytes.iter().position(|b| *b == b'[').unwrap() + 1;
        for item in array {
            while bytes
                .get(offset)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
            {
                offset += 1;
            }
            let mut parser =
                serde_json::Deserializer::from_slice(&bytes[offset..]).into_iter::<Value>();
            parser
                .next()
                .expect("validated array item")
                .expect("validated JSON");
            let end = offset + parser.byte_offset();
            if !item.is_null() && !item.is_string() {
                return type_error(
                    item,
                    if item.is_array() || item.is_object() {
                        offset + 1
                    } else {
                        end
                    },
                    "string",
                );
            }
            roles.push(item.as_str().unwrap_or_default().to_owned());
            offset = end;
        }
        Some(roles)
    } else {
        let start = bytes.iter().position(|b| !b.is_ascii_whitespace()).unwrap();
        let offset = if value.is_object() {
            start + 1
        } else {
            bytes
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .unwrap()
                + 1
        };
        return type_error(&value, offset, "[]string");
    };
    match scheduler.update_optional_roles(roles, storage, paths).await {
        Ok(()) => response(200, "Config is updated.".into(), false),
        Err(e @ ShuffleRegionError::InvalidRole(_)) => response(400, e.to_string(), false),
        Err(e) => response(500, e.to_string(), false),
    }
}
