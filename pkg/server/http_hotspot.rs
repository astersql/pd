// Copyright 2026 AsterSQL.
//! HTTP query parsing; the host owns all statistics and worker lifetimes.
use crate::{
    ClusterReadService,
    http_contract::{json_response, query_unescape},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Observation over the serving term's canonical caches.
#[derive(Clone, Debug)]
pub enum Query {
    /// Validate one store before parsing the next repeated query value, as in Go.
    Store(u64),
    Regions {
        write: bool,
        ids: Vec<u64>,
    },
    Stores,
    Buckets(Vec<u64>),
}
pub(crate) const PATHS: &[&str] = &[
    "/pd/api/v1/hotspot/regions/write",
    "/pd/api/v1/hotspot/regions/read",
    "/pd/api/v1/hotspot/stores",
    "/pd/api/v1/hotspot/buckets",
];
pub(crate) async fn respond(
    service: Option<&dyn ClusterReadService>,
    path: &str,
    query: &str,
) -> Response {
    let kind = path.rsplit('/').next().unwrap_or_default();
    let key = if kind == "buckets" {
        b"region_id".as_slice()
    } else {
        b"store_id".as_slice()
    };
    let mut ids = Vec::new();
    if kind != "stores" {
        for pair in query.split('&').filter(|p| !p.contains(';')) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let (Ok(k), Ok(v)) = (query_unescape(k), query_unescape(v)) else {
                continue;
            };
            if k != key {
                continue;
            }
            // strconv.ParseUint(base 10) accepts decimal digits only.
            let id = if !v.is_empty() && v.iter().all(u8::is_ascii_digit) {
                std::str::from_utf8(&v)
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            } else {
                None
            };
            let id = match id {
                Some(id) => id,
                None if kind == "buckets" => 0,
                None => {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        &format!("invalid store id: {}", String::from_utf8_lossy(&v)),
                    );
                }
            };
            if kind != "buckets" {
                let Some(service) = service else {
                    return unavailable_store();
                };
                if let Err(e) = service.hotspot(Query::Store(id)).await {
                    return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e);
                }
            }
            ids.push(id);
        }
    }
    let query = match kind {
        "stores" => Query::Stores,
        "buckets" => Query::Buckets(ids),
        _ => Query::Regions {
            write: kind == "write",
            ids,
        },
    };
    let Some(service) = service else {
        return match query {
            Query::Regions { .. } => cold(),
            _ => unavailable_store(),
        };
    };
    match service.hotspot(query).await {
        Ok(value) => json_response(StatusCode::OK, &value),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

fn cold() -> Response {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
    )
}

// Go returns a typed-nil SchedulerCluster and its outer recovery emits this
// response. Preserve the public error without triggering a panic or exposing a stack.
fn unavailable_store() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [("content-type", "text/plain; charset=utf-8")],
        "PANIC: runtime error: invalid memory address or nil pointer dereference\n",
    )
        .into_response()
}
