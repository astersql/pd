// Copyright 2026 AsterSQL.
//! Store label and balance-weight HTTP operations on the shared host service.
use crate::{
    PdServices,
    http_contract::{json_response, parse_uint64_field, query_unescape},
    http_store_state::StoreMutation,
    request_context,
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{OriginalUri, State},
    http::{Method, StatusCode},
    response::Response,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/store/{id}/label", post(handle).delete(handle))
        .route("/pd/api/v1/store/{id}/weight", post(handle))
        .route("/pd/api/v1/labels", get(handle))
        .with_state(Context { services, cancel })
}
pub(crate) fn input(msg: String) -> Response {
    let mut r = json_response(
        StatusCode::BAD_REQUEST,
        &json!({"code":"input","msg":msg,"data":{}}),
    );
    r.headers_mut()
        .insert("tidb-error-code", "input".parse().unwrap());
    r
}
async fn handle(
    State(s): State<Context>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    body: Body,
) -> Response {
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        match s.services.is_bootstrapped().await {
            Ok(true) => {}
            Ok(false) => {
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                );
            }
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
        if method == Method::GET {
            return match s.services.stores().await {
                Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
                Ok(stores) => {
                    let mut seen = std::collections::HashSet::new();
                    let mut labels = vec![];
                    for store in stores {
                        for l in store.labels {
                            if seen.insert(format!("{}{}", l.key, l.value).to_lowercase()) {
                                let mut v = json!({});
                                if !l.key.is_empty() {
                                    v["key"] = json!(l.key);
                                }
                                if !l.value.is_empty() {
                                    v["value"] = json!(l.value);
                                }
                                labels.push(v);
                            }
                        }
                    }
                    json_response(
                        StatusCode::OK,
                        &if labels.is_empty() {
                            Value::Null
                        } else {
                            json!(labels)
                        },
                    )
                }
            };
        }
        let raw = uri
            .path()
            .trim_start_matches("/pd/api/v1/store/")
            .split('/')
            .next()
            .unwrap();
        let raw =
            query_unescape(&raw.replace('+', "%2B")).unwrap_or_else(|_| raw.as_bytes().to_vec());
        let id = match parse_uint64_field(Some(&String::from_utf8_lossy(&raw)), "id") {
            Ok(n) => n,
            Err(crate::http_contract::HttpError::Input(e)) => return input(e),
            _ => unreachable!(),
        };
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        let value: Value = match serde_json::from_slice(&crate::http_contract::go_json_strings(
            &bytes,
        )) {
            Ok(v) => v,
            Err(e) => {
                let offset = if e.is_eof() {
                    bytes.len()
                } else {
                    bytes
                        .split_inclusive(|b| *b == b'\n')
                        .take(e.line().saturating_sub(1))
                        .map(<[u8]>::len)
                        .sum::<usize>()
                        + e.column()
                };
                let mut r = json_response(
                    StatusCode::BAD_REQUEST,
                    &json!({"code":"input","msg":crate::schedule_admin_balance_leader::syntax_error(&bytes,&e),"data":{"Offset":offset}}),
                );
                r.headers_mut()
                    .insert("tidb-error-code", "input".parse().unwrap());
                return r;
            }
        };
        let weight = uri.path().ends_with("/weight");
        if !value.is_null() && !value.is_object() && method != Method::DELETE {
            return type_error(
                &bytes,
                &value,
                if weight {
                    "map[string]interface {}"
                } else {
                    "map[string]string"
                },
                None,
            );
        }
        let (op, message) = if weight {
            for key in ["leader", "region"] {
                if value.get(key).is_none() {
                    return json_response(StatusCode::BAD_REQUEST, &format!("{key} weight unset"));
                }
            }
            let mut weights = vec![];
            for key in ["leader", "region"] {
                match value[key].as_f64() {
                    Some(n) if n >= 0.0 && n.is_finite() => weights.push(n),
                    _ => {
                        return json_response(
                            StatusCode::BAD_REQUEST,
                            &format!("bad format {key} weight"),
                        );
                    }
                }
            }
            (
                StoreMutation::Weight {
                    id,
                    leader: weights[0],
                    region: weights[1],
                },
                "The store's weight is updated.".to_owned(),
            )
        } else if method == Method::DELETE {
            let key = if value.is_null() {
                ""
            } else {
                match value.as_str() {
                    Some(v) => v,
                    None => return type_error(&bytes, &value, "string", None),
                }
            };
            if let Err(e) = astersql_pd_schedule_config::validate_label_key(key) {
                return input(e);
            }
            if key == "engine" {
                return input("can't modify label key \"engine\"".into());
            }
            (
                StoreMutation::DeleteLabel {
                    id,
                    key: key.into(),
                },
                format!("The label {key} is deleted for store {id}."),
            )
        } else {
            let empty = serde_json::Map::new();
            let map = if value.is_null() {
                &empty
            } else {
                match value.as_object() {
                    Some(m) => m,
                    None => unreachable!(),
                }
            };
            let mut labels = vec![];
            for (k, v) in map {
                let v = if v.is_null() {
                    ""
                } else {
                    match v.as_str() {
                        Some(v) => v,
                        None => return type_error(&bytes, v, "string", Some(k)),
                    }
                };
                labels.push(astersql_pd_core::StoreLabel {
                    key: k.clone(),
                    value: v.into(),
                });
            }
            if map.contains_key("engine") {
                return input("label key \"engine\" is reserved".into());
            }
            if let Err(e) = astersql_pd_schedule_config::validate_labels(
                labels.iter().map(|l| (l.key.as_str(), l.value.as_str())),
            ) {
                return input(e);
            }
            let force = uri
                .query()
                .unwrap_or_default()
                .split('&')
                .filter(|p| !p.contains(';'))
                .any(|p| {
                    query_unescape(p.split('=').next().unwrap())
                        .ok()
                        .is_some_and(|k| k == b"force")
                });
            (
                StoreMutation::Labels { id, labels, force },
                "The store's label is updated.".into(),
            )
        };
        match s.services.mutate_store(op).await {
            Ok(()) => json_response(StatusCode::OK, &message),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=token.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),r=work=>r}
}

pub(crate) fn type_error(
    bytes: &[u8],
    value: &Value,
    expected: &str,
    key: Option<&str>,
) -> Response {
    let kind = crate::schedule_admin_balance_leader::json_type(value);
    let (start, end) = key
        .and_then(|k| crate::http_schedule_config_write::field_bounds(bytes, k))
        .unwrap_or_else(|| {
            let start = bytes
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or(0);
            let end = bytes
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map(|n| n + 1)
                .unwrap_or(0);
            (start, end)
        });
    let offset = if value.is_array() || value.is_object() {
        start + 1
    } else {
        end
    };
    let mut r = json_response(
        StatusCode::BAD_REQUEST,
        &json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go value of type {expected}"),"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":"","Field":""}}),
    );
    r.headers_mut()
        .insert("tidb-error-code", "input".parse().unwrap());
    r
}
