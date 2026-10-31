// Copyright 2026 AsterSQL.
//! Read resolved timestamp reports from the serving cluster generation.
use crate::{
    PdServices,
    http_contract::{HttpError, json_response, parse_uint64_field, query_unescape},
    request_context,
};
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
/// Snapshot of the canonical cluster minimum and eligible store reports.
pub struct Observation {
    pub minimum: u64,
    pub stores: BTreeMap<u64, u64>,
}
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/min-resolved-ts", get(handle))
        .route("/pd/api/v1/min-resolved-ts/{id}", get(handle))
        .with_state(Context { services, cancel })
}
async fn handle(State(s): State<Context>, OriginalUri(uri): OriginalUri) -> Response {
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let bootstrapped = match s.services.is_bootstrapped().await {
            Ok(v) => v,
            Err(e) => return json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        if !bootstrapped {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &"[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
            );
        }
        let decode =
            |raw: &str| query_unescape(raw).map(|v| String::from_utf8_lossy(&v).into_owned());
        let path = uri.path().strip_prefix("/pd/api/v1/min-resolved-ts/");
        let scope = uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter_map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                if p.contains(';') {
                    return None;
                }
                (decode(k).ok()?.as_str() == "scope")
                    .then(|| decode(v).ok())
                    .flatten()
            })
            .next()
            .unwrap_or_default();
        let ids = if let Some(raw) = path {
            Some(
                decode(&raw.replace('+', "%2B"))
                    .and_then(|v| parse_uint64_field(Some(&v), "store_id"))
                    .map(|v| vec![v]),
            )
        } else if !scope.is_empty() && scope != "cluster" {
            Some(
                scope
                    .split(',')
                    .map(|v| parse_uint64_field(Some(v), "scope"))
                    .collect(),
            )
        } else {
            None
        };
        let ids = match ids.transpose() {
            Ok(v) => v,
            Err(HttpError::Input(e)) => return json_response(StatusCode::BAD_REQUEST, &e),
            Err(_) => unreachable!(),
        };
        let result = async {
            let observation = s.services.resolved_ts_observation().await?;
            let config = s.services.configuration(false).await?;
            let interval = config.pd_server.min_resolved_ts_persistence_interval;
            let mut minimum = observation.minimum;
            let mut stores = None;
            if let Some(ids) = ids {
                let values: BTreeMap<_, _> = ids
                    .into_iter()
                    .map(|id| (id, observation.stores.get(&id).copied().unwrap_or(u64::MAX)))
                    .collect();
                minimum = values.values().copied().min().unwrap_or(u64::MAX);
                if path.is_none() {
                    stores = Some(values);
                }
            } else if scope == "cluster" {
                stores = Some(observation.stores);
            }
            let mut body = serde_json::json!({
                "min_resolved_ts": minimum,
                "persist_interval": interval,
                "stores_min_resolved_ts": stores
            });
            if interval.as_nanos() != 0 {
                body["is_real_time"] = serde_json::json!(true);
            }
            Ok::<_, String>(body)
        }
        .await;
        match result {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        }
    };
    tokio::select! {biased;_=token.cancelled()=>json_response(StatusCode::INTERNAL_SERVER_ERROR,&"context canceled"),r=work=>r}
}
