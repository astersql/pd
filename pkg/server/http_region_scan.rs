// Copyright 2026 AsterSQL.
//! Region list HTTP adapters. Streams retain one immutable payload snapshot, so
//! concurrent heartbeats cannot make the advertised count disagree with the body.
use crate::{
    PdServices,
    http_contract::{json_response, query_unescape},
    request_context,
};
use astersql_pd_core::{RegionInfo, RegionRole};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Context {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    let mut router = Router::new()
        .route("/pd/api/v1/regions", get(handle))
        .route("/pd/api/v1/regions/check/hist-size", get(handle))
        .route("/pd/api/v1/regions/check/hist-keys", get(handle))
        .route("/pd/api/v1/stats/region", get(handle))
        .route("/pd/api/v1/regions/key", get(handle))
        .route("/pd/api/v1/regions/count", get(handle))
        .route("/pd/api/v1/regions/store/{id}", get(handle));
    for path in crate::http_region_check::PATHS {
        router = router.route(path, get(handle));
    }
    for dimension in crate::http_region_top::DIMENSIONS {
        router = router.route(&format!("/pd/api/v1/regions/{dimension}"), get(handle));
    }
    router.with_state(Context { services, cancel })
}
fn error(code: StatusCode, message: impl AsRef<str>) -> Response {
    json_response(code, &message.as_ref())
}
pub(crate) fn atoi(raw: &str) -> Result<i64, String> {
    raw.parse().map_err(|_| {
        let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
        let reason = if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            "value out of range"
        } else {
            "invalid syntax"
        };
        format!("strconv.Atoi: parsing {raw:?}: {reason}")
    })
}
fn unhex(key: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for pair in key.chunks(2) {
        let mut byte = 0;
        for c in pair {
            let Some(n) = (*c as char).to_digit(16) else {
                return Err(if (*c as char).is_control() {
                    format!("encoding/hex: invalid byte: U+{c:04X}")
                } else {
                    format!("encoding/hex: invalid byte: U+{c:04X} '{}'", *c as char)
                });
            };
            byte = byte * 16 + n as u8;
        }
        if pair.len() != 2 {
            return Err("encoding/hex: odd length hex string".into());
        }
        out.push(byte);
    }
    Ok(out)
}
/// Pull-driven pages: no producer task, channel, or detached scan survives a
/// dropped HTTP body. Reuse the canonical response object across page boundaries.
pub(crate) fn stream_regions(regions: Vec<Arc<RegionInfo>>, cancel: CancellationToken) -> Body {
    let count = regions.len();
    let state = (
        regions.into_iter(),
        astersql_pd_response::RegionInfo::default(),
        0usize,
        false,
        cancel,
    );
    Body::from_stream(futures_util::stream::unfold(
        state,
        move |(mut regions, mut response, mut index, done, cancel)| async move {
            if done {
                return None;
            }
            tokio::task::yield_now().await;
            let mut bytes = if index == 0 {
                format!("{{\"count\":{count},\"regions\":[").into_bytes()
            } else {
                Vec::new()
            };
            for _ in 0..64 {
                if cancel.is_cancelled() {
                    return Some((
                        Err::<Vec<u8>, std::io::Error>(std::io::Error::other("context canceled")),
                        (regions, response, index, true, cancel),
                    ));
                }
                let Some(region) = regions.next() else {
                    break;
                };
                if index > 0 {
                    bytes.push(b',');
                }
                astersql_pd_response::init_region(Some(&region), &mut response);
                serde_json::to_writer(&mut bytes, &response).expect("region JSON");
                index += 1;
            }
            let done = regions.len() == 0;
            if done {
                bytes.extend_from_slice(b"]}");
            }
            Some((Ok(bytes), (regions, response, index, done, cancel)))
        },
    ))
}
async fn handle(State(s): State<Context>, OriginalUri(uri): OriginalUri) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        // net/url ignores malformed pairs and keeps the first value.
        let mut query = std::collections::HashMap::new();
        for pair in uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter(|p| !p.contains(';'))
        {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if let (Ok(k), Ok(v)) = (query_unescape(k), query_unescape(v)) {
                query.entry(k).or_insert(v);
            }
        }
        if uri.path().ends_with("/regions/replicated")
            && (!query.contains_key(b"startKey".as_slice())
                || !query.contains_key(b"endKey".as_slice()))
        {
            return crate::http_region_check::not_found();
        }
        let follower = s.services.healthy().await.is_err_and(|e| e == "not leader");
        let cold =
            "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first";
        // Cluster-only observations reject explicitly local follower handling;
        // histograms and range holes may use the synchronized cache.
        if follower
            && (uri.path() == "/pd/api/v1/stats/region"
                || (crate::http_region_check::PATHS.contains(&uri.path())
                    && !uri.path().ends_with("/range-holes")))
        {
            return error(StatusCode::INTERNAL_SERVER_ERROR, cold);
        }
        if !follower {
            match s.services.is_bootstrapped().await {
                Ok(true) => {}
                Ok(false) => return error(StatusCode::INTERNAL_SERVER_ERROR, cold),
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            }
        }
        let Some(service) = (if follower {
            s.services.follower_read_service()
        } else {
            s.services.cluster_read_service()
        }) else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "cluster reads are not configured",
            );
        };
        if follower {
            match service.query_regions(&[], &[], &[], false).await {
                Ok(Some(_)) => {}
                Ok(None) => return error(StatusCode::INTERNAL_SERVER_ERROR, cold),
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            }
        }
        if uri.path().ends_with("/count") {
            return match service.region_count().await {
                Ok(count) => json_response(
                    StatusCode::OK,
                    &serde_json::json!({"count":count,"regions":null}),
                ),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
        }
        if crate::http_region_check::PATHS.contains(&uri.path()) {
            return crate::http_region_check::respond(service, uri.path(), &query, &cancel).await;
        }
        if uri.path().ends_with("/hist-size")
            || uri.path().ends_with("/hist-keys")
            || uri.path().ends_with("/stats/region")
        {
            return crate::http_region_stats::respond(service, uri.path(), &query, &cancel).await;
        }
        let get = |k: &str| query.get(k.as_bytes()).cloned().unwrap_or_default();
        let dimension = uri
            .path()
            .strip_prefix("/pd/api/v1/regions/")
            .unwrap_or_default();
        let regions = if crate::http_region_top::DIMENSIONS.contains(&dimension) {
            let raw = get("limit");
            let limit = if raw.is_empty() {
                16
            } else {
                match atoi(&String::from_utf8_lossy(&raw)) {
                    Ok(v) => v,
                    Err(e) => return error(StatusCode::BAD_REQUEST, e),
                }
            };
            match service.region_list(None).await {
                Ok(regions) => crate::http_region_top::select(regions, dimension, limit, &cancel)
                    .await
                    .map_err(str::to_owned),
                Err(e) => Err(e),
            }
        } else if uri.path().ends_with("/key") {
            let mut start = get("key");
            let mut end = get("end_key");
            if get("format") == b"hex" {
                start = match unhex(&start) {
                    Ok(v) => v,
                    Err(e) => return error(StatusCode::BAD_REQUEST, e),
                };
                end = match unhex(&end) {
                    Ok(v) => v,
                    Err(e) => return error(StatusCode::BAD_REQUEST, e),
                };
            }
            let limit = get("limit");
            let limit = if limit.is_empty() {
                16
            } else {
                match atoi(&String::from_utf8_lossy(&limit)) {
                    Ok(v) => v.min(10240),
                    Err(e) => return error(StatusCode::BAD_REQUEST, e),
                }
            };
            service.scan_regions(&start, &end, limit as isize).await
        } else {
            let store = if uri.path().contains("/store/") {
                let raw = uri.path().rsplit('/').next().unwrap_or_default();
                let raw = query_unescape(&raw.replace('+', "%2B")).unwrap_or_default();
                let id = match atoi(&String::from_utf8_lossy(&raw)) {
                    Ok(v) => v as u64,
                    Err(e) => return error(StatusCode::BAD_REQUEST, e),
                };
                let typ = get("type");
                let role = match typ.as_slice() {
                    b"" | b"all" => None,
                    b"leader" => Some(RegionRole::Leader),
                    b"follower" => Some(RegionRole::Follower),
                    b"learner" => Some(RegionRole::Learner),
                    b"pending" => Some(RegionRole::Pending),
                    b"witness" => Some(RegionRole::Witness),
                    _ => {
                        return error(
                            StatusCode::BAD_REQUEST,
                            format!(
                                "unknown sub tree region type {}",
                                String::from_utf8_lossy(&typ)
                            ),
                        );
                    }
                };
                Some((id, role))
            } else {
                None
            };
            service.region_list(store).await
        };
        match regions {
            Ok(regions) => (
                StatusCode::OK,
                [("content-type", "application/octet-stream")],
                stream_regions(regions, cancel.clone()),
            )
                .into_response(),
            Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
        }
    };
    tokio::select! {biased; _=cancel.cancelled()=>error(StatusCode::INTERNAL_SERVER_ERROR,"context canceled"), r=work=>r}
}
