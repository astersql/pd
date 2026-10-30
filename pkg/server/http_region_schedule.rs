// Copyright 2026 AsterSQL.
//! Region scheduling HTTP adapters over the host's existing coordinator.
use crate::{
    PdServices,
    grpc_scatter::{ScatterError, ScatterRequest},
    grpc_split_regions::SplitRequest,
    http_contract::json_response,
};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
/// Decoded ranges for the canonical checker pending queue.
#[derive(Clone, Debug)]
pub struct AccelerateRequest {
    pub starts: Vec<Vec<u8>>,
    pub ends: Vec<Vec<u8>>,
    pub limit: isize,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    let mut router = Router::new();
    for path in [
        "accelerate-schedule",
        "accelerate-schedule/batch",
        "scatter",
        "split",
    ] {
        router = router.route(&format!("/pd/api/v1/regions/{path}"), any(handle));
    }
    router.with_state((services, cancel))
}
fn error(code: StatusCode, e: impl AsRef<str>) -> Response {
    json_response(code, &e.as_ref())
}
fn hex(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_bytes().chunks(2) {
        let mut n = 0;
        for c in pair {
            let Some(d) = (*c as char).to_digit(16) else {
                return Err(if (32..=126).contains(c) || (*c >= 161 && *c != 173) {
                    format!("encoding/hex: invalid byte: U+{c:04X} '{}'", *c as char)
                } else {
                    format!("encoding/hex: invalid byte: U+{c:04X}")
                });
            };
            n = n * 16 + d as u8;
        }
        if pair.len() != 2 {
            return Err("encoding/hex: odd length hex string".into());
        }
        out.push(n);
    }
    Ok(out)
}
fn range(v: &Value, batch: bool) -> Result<(Vec<u8>, Vec<u8>, String, String), String> {
    let parse = |name: &str| -> Result<(Vec<u8>, String), String> {
        let Some(value) = v.get(name) else {
            return Err(if batch {
                format!("missing {name}")
            } else {
                "start_key or end_key is not string".into()
            });
        };
        let Some(s) = value.as_str() else {
            return Err(if batch {
                format!("bad format {name}")
            } else {
                "start_key or end_key is not string".into()
            });
        };
        Ok((
            hex(s).map_err(|e| {
                if batch {
                    format!("split key {name} is not in hex format")
                } else {
                    e
                }
            })?,
            s.into(),
        ))
    };
    if !batch && (v["start_key"].as_str().is_none() || v["end_key"].as_str().is_none()) {
        return Err("start_key or end_key is not string".into());
    }
    let (a, ah) = parse("start_key")?;
    let (b, bh) = parse("end_key")?;
    Ok((a, b, ah, bh))
}
async fn handle(
    State((services, cancel)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    if method == Method::OPTIONS {
        return StatusCode::OK.into_response();
    }
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| cancel.child_token());
    let _guard = cancel.clone().drop_guard();
    let work = async {
        // Go mounts all four routes below newClusterMiddleware, before JSON decoding.
        if let Err(e) = bootstrap(&*services).await {
            return error(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
        let batch = uri.path().ends_with("/batch");
        let bytes = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(v) => v,
            Err(_) => return crate::http_contract::HttpError::Internal.into_response(),
        };
        let input: Value = match serde_json::from_slice(&crate::http_contract::go_json_strings(
            &bytes,
        )) {
            Ok(v) => v,
            Err(e) => {
                return input_error(
                    crate::schedule_admin_balance_leader::syntax_error(&bytes, &e),
                    json!({"Offset":if e.is_eof(){bytes.len()}else{bytes.split_inclusive(|b|*b==b'\n').take(e.line().saturating_sub(1)).map(<[u8]>::len).sum::<usize>()+e.column()}}),
                );
            }
        };
        // Typed map/slice decoding preserves errors for scalar batch elements too.
        let normalized = crate::http_contract::go_json_strings(&bytes);
        let invalid = if batch {
            if let Some(items) = input.as_array() {
                items.iter().find(|v| !v.is_null() && !v.is_object())
            } else if input.is_null() {
                None
            } else {
                Some(&input)
            }
        } else if input.is_object() || input.is_null() {
            None
        } else {
            Some(&input)
        };
        if let Some(value) = invalid {
            let typed_error = if batch {
                serde_json::from_slice::<Option<Vec<Option<serde_json::Map<String, Value>>>>>(
                    &normalized,
                )
                .err()
            } else {
                serde_json::from_slice::<Option<serde_json::Map<String, Value>>>(&normalized).err()
            };
            let e = typed_error.expect("non-map JSON value must fail typed map decoding");
            let kind = crate::schedule_admin_balance_leader::json_type(value);
            let expected = if batch && !input.is_array() {
                "[]map[string]interface {}"
            } else {
                "map[string]interface {}"
            };
            let offset = bytes
                .split_inclusive(|b| *b == b'\n')
                .take(e.line().saturating_sub(1))
                .map(<[u8]>::len)
                .sum::<usize>()
                + e.column()
                + usize::from(value.is_array() || value.is_object());
            return input_error(
                format!("json: cannot unmarshal {kind} into Go value of type {expected}"),
                json!({"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}),
            );
        }
        if uri.path().contains("/accelerate-schedule") {
            if !batch
                && (input["start_key"].as_str().is_none() || input["end_key"].as_str().is_none())
            {
                return error(
                    StatusCode::BAD_REQUEST,
                    "start_key or end_key is not string",
                );
            }
            let mut limit = 256i64;
            for p in uri
                .query()
                .unwrap_or_default()
                .split('&')
                .filter(|p| !p.contains(';'))
            {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                let (Ok(k), Ok(v)) = (
                    crate::http_contract::query_unescape(k),
                    crate::http_contract::query_unescape(v),
                ) else {
                    continue;
                };
                if k == b"limit" {
                    let raw = String::from_utf8_lossy(&v);
                    if !raw.is_empty() {
                        limit = match crate::http_region_scan::atoi(&raw) {
                            Ok(n) => n,
                            Err(e) => return error(StatusCode::BAD_REQUEST, e),
                        };
                    }
                    break;
                }
            }
            let mut request = AccelerateRequest {
                starts: vec![],
                ends: vec![],
                limit: limit.min(10240) as isize,
            };
            let mut message = if batch {
                "Accelerate regions scheduling in given ranges: ".to_owned()
            } else {
                String::new()
            };
            let items = if batch {
                input.as_array().cloned().unwrap_or_default()
            } else {
                vec![input]
            };
            for item in items {
                let (a, b, ah, bh) = match range(&item, batch) {
                    Ok(v) => v,
                    Err(e) => {
                        return error(
                            if batch || e == "start_key or end_key is not string" {
                                StatusCode::BAD_REQUEST
                            } else {
                                StatusCode::INTERNAL_SERVER_ERROR
                            },
                            e,
                        );
                    }
                };
                request.starts.push(a);
                request.ends.push(b);
                if batch {
                    message.push_str(&format!("[{ah},{bh}), "));
                } else {
                    message = format!("Accelerate regions scheduling in a given range [{ah},{bh})");
                }
            }
            return match services.accelerate_regions(request).await {
                Ok(()) => (
                    StatusCode::OK,
                    [("content-type", "text/plain; charset=UTF-8")],
                    message,
                )
                    .into_response(),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
        }
        let retries = input["retry_limit"]
            .as_f64()
            .map(astersql_pd_schedule_handler::go_json_i64)
            .unwrap_or(5);
        if uri.path().ends_with("/split") {
            let Some(keys) = input.get("split_keys") else {
                return error(StatusCode::BAD_REQUEST, "split_keys should be provided.");
            };
            let Some(keys) = keys.as_array() else {
                return error(StatusCode::BAD_REQUEST, "split_keys should be an array.");
            };
            if keys.is_empty() {
                return error(StatusCode::BAD_REQUEST, "empty split keys.");
            }
            if keys.iter().any(|k| !k.is_string()) {
                return error(
                    StatusCode::BAD_REQUEST,
                    "split_keys should contain only strings.",
                );
            }
            let keys = match keys
                .iter()
                .map(|k| hex(k.as_str().unwrap()))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(v) => v,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
            return match services
                .split_regions(SplitRequest {
                    keys,
                    retries,
                    cancellation: cancel.clone(),
                })
                .await
            {
                Ok(v) => json_response(
                    StatusCode::OK,
                    &json!({"processed-percentage":v.percentage,"regions-id":v.ids}),
                ),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
        }
        let keys = if input["start_key"].is_string() && input["end_key"].is_string() {
            match range(&input, false) {
                Ok((a, b, _, _)) => Some((a, b)),
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            }
        } else {
            None
        };
        let mut ids = vec![];
        if keys.is_none() {
            let Some(values) = input["regions_id"].as_array() else {
                return error(StatusCode::INTERNAL_SERVER_ERROR, "regions_id is invalid");
            };
            for v in values {
                let Some(id) = astersql_pd_schedule_handler::go_json_u64(v) else {
                    return error(StatusCode::INTERNAL_SERVER_ERROR, "regions_id is invalid");
                };
                ids.push(id);
            }
        }
        if ids.is_empty() && keys.is_none() {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "[PD:scatter:ErrEmptyRegion]empty region",
            );
        }
        match services
            .scatter(ScatterRequest {
                range: keys
                    .map(|(start_key, end_key)| astersql_pd_core::KeyRange { start_key, end_key }),
                ids,
                single: 0,
                fallback: None,
                group: input["group"].as_str().unwrap_or_default().into(),
                retries: retries.clamp(-1, 30) as i32,
                skip_store_limit: false,
            })
            .await
        {
            Ok(v) => json_response(
                StatusCode::OK,
                &json!({"processed-percentage":v.percentage}),
            ),
            Err(e) => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                match e {
                    ScatterError::Other(e) => e,
                    ScatterError::Missing => {
                        "[PD:region:ErrRegionNotFound]region %v not found".into()
                    }
                    ScatterError::Rejected => {
                        "operator canceled because cannot add an operator to the execute queue"
                            .into()
                    }
                },
            ),
        }
    };
    tokio::select! {biased;_=cancel.cancelled()=>error(StatusCode::INTERNAL_SERVER_ERROR,"server not started"),r=work=>r}
}
async fn bootstrap(services: &dyn PdServices) -> Result<(), String> {
    if services.is_bootstrapped().await? {
        Ok(())
    } else {
        Err(
            "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
                .into(),
        )
    }
}
fn input_error(message: String, data: Value) -> Response {
    let mut response = json_response(
        StatusCode::BAD_REQUEST,
        &json!({"code":"input","msg":message,"data":data}),
    );
    response
        .headers_mut()
        .insert("tidb-error-code", "input".parse().unwrap());
    response
}
