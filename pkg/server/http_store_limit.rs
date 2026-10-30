// Copyright 2026 AsterSQL.
//! Store limit and progress HTTP adaptation over the canonical host state.
use crate::{
    PdServices,
    http_contract::{json_response, parse_uint64_field, query_unescape},
    http_store_labels::{input, type_error},
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
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/store/{id}/limit", post(handle))
        .route("/pd/api/v1/stores/limit", get(handle).post(handle))
        .route("/pd/api/v1/stores/progress", get(handle))
        .with_state(Context { services, cancel })
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
        let mut query = BTreeMap::new();
        for pair in uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter(|p| !p.contains(';'))
        {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if let (Ok(k), Ok(v)) = (query_unescape(k), query_unescape(v)) {
                query
                    .entry(String::from_utf8_lossy(&k).into_owned())
                    .or_insert(String::from_utf8_lossy(&v).into_owned());
            }
        }
        let q = |key: &str| query.get(key).map(String::as_str).unwrap_or_default();
        if uri.path().ends_with("/progress") {
            let id = if !q("id").is_empty() {
                match parse_uint64_field(Some(q("id")), "id") {
                    Ok(n) => Some(n),
                    Err(crate::http_contract::HttpError::Input(e)) => {
                        let mut r = json_response(
                            StatusCode::BAD_REQUEST,
                            &json!({"code":"input","msg":e,"data":{"Func":"ParseUint","Num":q("id"),"Err":{}}}),
                        );
                        r.headers_mut()
                            .insert("tidb-error-code", "input".parse().unwrap());
                        return r;
                    }
                    _ => unreachable!(),
                }
            } else {
                None
            };
            if id.is_none() && q("action").is_empty() {
                return json_response(StatusCode::BAD_REQUEST, &"need query parameters");
            }
            return match s.services.store_progress(id, q("action").into()).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) if e.starts_with("json: unsupported value: ") => {
                    use axum::response::IntoResponse;
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [
                            ("content-type", "text/plain; charset=utf-8"),
                            ("x-content-type-options", "nosniff"),
                        ],
                        format!("{e}\n"),
                    )
                        .into_response()
                }
                Err(e) => json_response(StatusCode::NOT_FOUND, &e),
            };
        }
        if method == Method::POST && !q("ttlSecond").is_empty() {
            return json_response(StatusCode::BAD_REQUEST, &"ttlSecond is deprecated");
        }
        let cfg = match s.services.configuration(false).await {
            Ok(c) => c.schedule,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        let single = uri.path().starts_with("/pd/api/v1/store/");
        if cfg.store_limit_version != "v1" {
            return json_response(
                StatusCode::BAD_REQUEST,
                &format!(
                    "current store limit version:{} not support {} limit",
                    cfg.store_limit_version,
                    if single { "set" } else { "get" }
                ),
            );
        }
        if method == Method::GET {
            let include = match q("include_tombstone") {
                "" | "0" | "f" | "F" | "false" | "False" | "FALSE" => false,
                "1" | "t" | "T" | "true" | "True" | "TRUE" => true,
                v => {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        &format!("strconv.ParseBool: parsing {v:?}: invalid syntax"),
                    );
                }
            };
            let mut limits = cfg.store_limit;
            if !include {
                let stores = match s.services.stores().await {
                    Ok(v) => v,
                    Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
                };
                let active: std::collections::HashSet<_> = stores
                    .iter()
                    .filter(|s| s.state != astersql_pd_core::StoreState::Tombstone)
                    .map(|s| s.id)
                    .collect();
                let map = limits.get_or_insert_with(Default::default);
                map.retain(|id, _| active.contains(id));
            }
            return json_response(StatusCode::OK, &limits);
        }
        let id = if single {
            let raw = uri
                .path()
                .trim_start_matches("/pd/api/v1/store/")
                .split('/')
                .next()
                .unwrap();
            let raw = query_unescape(&raw.replace('+', "%2B"))
                .unwrap_or_else(|_| raw.as_bytes().to_vec());
            let id = match parse_uint64_field(Some(&String::from_utf8_lossy(&raw)), "id") {
                Ok(n) => n,
                Err(crate::http_contract::HttpError::Input(e)) => return input(e),
                _ => unreachable!(),
            };
            match s.services.stores().await {
                Ok(stores) if stores.iter().any(|s| s.id == id) => {}
                Ok(_) => {
                    return json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("[PD:core:ErrStoreNotFound]store {id} not found"),
                    );
                }
                Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
            }
            Some(id)
        } else {
            None
        };
        let bytes = match to_bytes(body, usize::MAX).await {
            Ok(v) => v,
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
                if e.to_string().starts_with("number out of range") {
                    let end = offset.min(bytes.len());
                    let start = bytes[..end]
                        .iter()
                        .rposition(|b| !matches!(b, b'0'..=b'9' | b'e' | b'E' | b'+' | b'-' | b'.'))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let number = String::from_utf8_lossy(&bytes[start..end]);
                    let kind = format!("number {number}");
                    let mut r = json_response(
                        StatusCode::BAD_REQUEST,
                        &json!({"code":"input","msg":format!("json: cannot unmarshal {kind} into Go value of type float64"),"data":{"Value":kind,"Type":{},"Offset":end+1,"Struct":"","Field":""}}),
                    );
                    r.headers_mut()
                        .insert("tidb-error-code", "input".parse().unwrap());
                    return r;
                }
                let mut r = json_response(
                    StatusCode::BAD_REQUEST,
                    &json!({"code":"input","msg":crate::schedule_admin_balance_leader::syntax_error(&bytes,&e),"data":{"Offset":offset}}),
                );
                r.headers_mut()
                    .insert("tidb-error-code", "input".parse().unwrap());
                return r;
            }
        };
        if !value.is_object() && !value.is_null() {
            return type_error(&bytes, &value, "map[string]interface {}", None);
        }
        let Some(rate) = value.get("rate") else {
            return json_response(StatusCode::BAD_REQUEST, &"rate unset");
        };
        let rate = match rate.as_f64() {
            Some(n) if n > 0.0 && n.is_finite() => n,
            _ => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    &"invalid rate which should be larger than 0",
                );
            }
        };
        let types = match value.get("type") {
            None => vec!["add-peer".into(), "remove-peer".into()],
            Some(Value::String(t)) => match t.as_str() {
                "" => vec!["add-peer".into()],
                "add-peer" | "remove-peer" | "transfer-leader-in" | "send-snapshot" => {
                    vec![t.clone()]
                }
                _ => return json_response(StatusCode::BAD_REQUEST, &"unknown type"),
            },
            _ => return json_response(StatusCode::BAD_REQUEST, &"bad format type"),
        };
        let labels = if !single {
            match value.get("labels") {
                None => None,
                Some(Value::Object(m)) => {
                    let mut labels = vec![];
                    for (k, v) in m {
                        let Some(v) = v.as_str() else {
                            return json_response(
                                StatusCode::BAD_REQUEST,
                                &"invalid label value which should be a string",
                            );
                        };
                        labels.push(astersql_pd_core::StoreLabel {
                            key: k.clone(),
                            value: v.into(),
                        });
                    }
                    if let Err(e) = astersql_pd_schedule_config::validate_labels(
                        labels.iter().map(|l| (l.key.as_str(), l.value.as_str())),
                    ) {
                        return input(e);
                    }
                    Some(labels)
                }
                _ => {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        &"invalid labels which should be an object",
                    );
                }
            }
        } else {
            None
        };
        match s
            .services
            .mutate_store(StoreMutation::Limit {
                id,
                labels,
                rate,
                types,
            })
            .await
        {
            Ok(()) => json_response(
                StatusCode::OK,
                &if single {
                    "The store's limit is updated."
                } else {
                    "Set store limit successfully."
                },
            ),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=token.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"server not started"),r=work=>r}
}
