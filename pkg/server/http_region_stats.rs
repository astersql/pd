// Copyright 2026 AsterSQL.
//! Statistics JSON and histogram transport; aggregation remains in canonical domain APIs.
use crate::{ClusterReadService, http_contract::json_response};
use axum::{http::StatusCode, response::Response};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use tokio_util::sync::CancellationToken;
pub(crate) async fn respond(
    service: &dyn ClusterReadService,
    path: &str,
    query: &HashMap<Vec<u8>, Vec<u8>>,
    cancel: &CancellationToken,
) -> Response {
    let get = |key: &str| {
        query
            .get(key.as_bytes())
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    if path.ends_with("/stats/region") {
        let count = query.contains_key(b"count".as_slice());
        return match service
            .region_statistics(
                get("start_key"),
                get("end_key"),
                count,
                query.contains_key(b"use_hot".as_slice()),
                &String::from_utf8_lossy(get("engine")),
            )
            .await
        {
            Ok(stats) => json_response(StatusCode::OK, &encode(&stats, count)),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
    }
    let keys = path.ends_with("/hist-keys");
    let mut bound = if keys { 1000i64 } else { 1 };
    let raw = get("bound");
    if !raw.is_empty() {
        match crate::http_region_scan::atoi(&String::from_utf8_lossy(raw)) {
            Ok(n) => bound = bound.max(n),
            Err(e) => return json_response(StatusCode::BAD_REQUEST, &e),
        }
    }
    let regions = match service.region_list(None).await {
        Ok(v) => v,
        Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match histogram(&regions, keys, bound, cancel).await {
        Ok(rows) => json_response(StatusCode::OK, &rows),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}
/// Consume one canonical snapshot; integer operations match Go int64 arithmetic.
pub(crate) async fn histogram(
    regions: &[std::sync::Arc<astersql_pd_core::RegionInfo>],
    keys: bool,
    bound: i64,
    cancel: &CancellationToken,
) -> Result<Vec<Value>, &'static str> {
    if cancel.is_cancelled() {
        return Err("context canceled");
    }
    let mut hist = BTreeMap::<i64, i64>::new();
    for (i, r) in regions.iter().enumerate() {
        if i % 256 == 0 {
            tokio::task::yield_now().await;
        }
        if cancel.is_cancelled() {
            return Err("context canceled");
        }
        let value = if keys {
            r.stats().approximate_keys
        } else {
            r.stats().approximate_size
        };
        *hist.entry(value / bound).or_default() += 1;
    }
    let mut rows: Vec<_> = hist
        .into_iter()
        .map(|(multiple, count)| {
            json!({
                "start": multiple.wrapping_mul(bound),
                "end": multiple.wrapping_add(1).wrapping_mul(bound).wrapping_sub(1),
                "count": count,
            })
        })
        .collect();
    rows.sort_by_key(|r| r["start"].as_i64().unwrap());
    Ok(rows)
}
fn encode(s: &astersql_pd_statistics::RegionStats, count: bool) -> Value {
    let mut v = json!({
        "count": s.count,
        "empty_count": s.empty_count,
        "storage_size": s.storage_size,
        "user_storage_size": s.user_storage_size,
        "user_ia_storage_size": s.user_ia_storage_size,
        "user_columnar_storage_size": s.user_columnar_storage_size,
        "storage_keys": s.storage_keys,
    });
    macro_rules! required {($($f:ident),*)=>{$(v[stringify!($f)]=if count {Value::Null}else{json!(s.$f)};)*};}
    required!(
        store_leader_count,
        store_peer_count,
        store_leader_size,
        store_leader_keys,
        store_peer_size
    );
    macro_rules! optional {($($f:ident),*)=>{$(if !s.$f.is_empty(){v[stringify!($f)]=json!(s.$f);})*};}
    optional!(
        store_peer_keys,
        store_write_bytes,
        store_write_keys,
        store_write_query,
        store_leader_read_bytes,
        store_leader_read_keys,
        store_leader_read_query,
        store_peer_read_bytes,
        store_peer_read_keys,
        store_peer_read_query,
        store_engine
    );
    v
}
