// Copyright 2026 AsterSQL.
//! Read-only classic operator HTTP, backed by the host's canonical coordinator.
use crate::{
    PdServices,
    http_contract::{json_response, query_unescape},
    request_context,
};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
/// Transport-independent query; timestamps are signed Unix seconds as in Go.
#[derive(Clone, Debug)]
pub enum Query {
    List {
        kinds: Option<Vec<String>>,
        object: bool,
    },
    Region(u64),
    Records(Option<i64>),
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/operators", any(handle))
        .route("/pd/api/v1/operators/records", any(handle))
        .route("/pd/api/v1/operators/{id}", any(handle))
        .with_state((services, cancel))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: axum::body::Body,
) -> Response {
    if method == Method::POST || method == Method::DELETE {
        return crate::http_operator_write::handle(services, cancel, uri, method, body).await;
    }
    if method == Method::OPTIONS {
        return StatusCode::OK.into_response();
    }
    if method != Method::GET {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let work = async {
        let pairs: Vec<_> = uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter(|p| !p.contains(';'))
            .filter_map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                Some((query_unescape(k).ok()?, query_unescape(v).ok()?))
            })
            .collect();
        let query = match uri
            .path()
            .strip_prefix("/pd/api/v1/operators")
            .unwrap_or_default()
        {
            "" => Query::List {
                kinds: {
                    let kinds: Vec<_> = pairs
                        .iter()
                        .filter(|(k, _)| k == b"kind")
                        .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
                        .collect();
                    if kinds.is_empty() { None } else { Some(kinds) }
                },
                object: pairs.iter().any(|(k, _)| k == b"object"),
            },
            "/records" => {
                let from = pairs
                    .iter()
                    .find(|(k, _)| k == b"from")
                    .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
                    .unwrap_or_default();
                let value = if from.is_empty() {
                    None
                } else {
                    let negative = from.starts_with('-');
                    let digits = from.strip_prefix(['+', '-']).unwrap_or(&from);
                    let parsed = decimal(digits).and_then(|n| {
                        let limit = if negative {
                            1u64 << 63
                        } else {
                            i64::MAX as u64
                        };
                        if n > limit {
                            Err("value out of range")
                        } else {
                            Ok(if negative {
                                (n as i64).wrapping_neg()
                            } else {
                                n as i64
                            })
                        }
                    });
                    match parsed {
                        Ok(n) => Some(n),
                        Err(reason) => return number_error("ParseInt", &from, reason),
                    }
                };
                Query::Records(value)
            }
            path => {
                let raw = &path[1..];
                // Path percent escapes are decoded exactly once, without query '+' rules.
                let decoded = query_unescape(&raw.replace('+', "%2B"))
                    .unwrap_or_else(|_| raw.as_bytes().to_vec());
                let text = String::from_utf8_lossy(&decoded);
                match decimal(&text) {
                    Ok(id) => Query::Region(id),
                    Err(reason) => return number_error("ParseUint", &text, reason),
                }
            }
        };
        // Unknown kinds never call a Go Handler method, even before bootstrap.
        if let Query::List {
            kinds: Some(kinds),
            object,
        } = &query
            && kinds
                .iter()
                .all(|k| !matches!(k.as_str(), "admin" | "leader" | "region" | "waiting"))
        {
            return json_response(
                StatusCode::OK,
                &if *object {
                    serde_json::json!([])
                } else {
                    serde_json::Value::Null
                },
            );
        }
        match services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        match services.operator_read(query).await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),response=work=>response}
}

// strconv reports the first bad digit or overflow, rather than prevalidating
// the whole input. ParseInt scans uint64 before checking its signed bound.
pub(crate) fn decimal(text: &str) -> Result<u64, &'static str> {
    if text.is_empty() {
        return Err("invalid syntax");
    }
    let mut n = 0u64;
    for b in text.bytes() {
        if !b.is_ascii_digit() {
            return Err("invalid syntax");
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u64))
            .ok_or("value out of range")?;
    }
    Ok(n)
}
pub(crate) fn number_error(method: &str, text: &str, reason: &str) -> Response {
    json_response(
        StatusCode::BAD_REQUEST,
        &format!(
            "strconv.{method}: parsing {}: {reason}",
            serde_json::to_string(text).unwrap()
        ),
    )
}
