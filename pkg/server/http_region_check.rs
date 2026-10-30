// Copyright 2026 AsterSQL.
//! HTTP observations over the host's canonical anomaly index and coordinator.
use crate::{ClusterReadService, http_contract::json_response};
use astersql_pd_statistics::RegionStatisticType::*;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
pub(crate) const PATHS: &[&str] = &[
    "/pd/api/v1/regions/check/miss-peer",
    "/pd/api/v1/regions/check/extra-peer",
    "/pd/api/v1/regions/check/pending-peer",
    "/pd/api/v1/regions/check/down-peer",
    "/pd/api/v1/regions/check/learner-peer",
    "/pd/api/v1/regions/check/offline-peer",
    "/pd/api/v1/regions/check/empty-region",
    "/pd/api/v1/regions/check/oversized-region",
    "/pd/api/v1/regions/check/undersized-region",
    "/pd/api/v1/regions/range-holes",
    "/pd/api/v1/regions/replicated",
];
pub(crate) async fn respond(
    service: &dyn ClusterReadService,
    path: &str,
    query: &HashMap<Vec<u8>, Vec<u8>>,
    cancel: &CancellationToken,
) -> Response {
    let name = path.rsplit('/').next().unwrap_or_default();
    if name == "range-holes" {
        return match service.region_range_holes().await {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
    }
    if name == "replicated" {
        // Gorilla registers this route only when both query keys match.
        if !query.contains_key(b"startKey".as_slice()) || !query.contains_key(b"endKey".as_slice())
        {
            return not_found();
        }
        for key in [b"startKey".as_slice(), b"endKey".as_slice()] {
            if let Err(e) = validate_hex(query.get(key).unwrap()) {
                return json_response(StatusCode::BAD_REQUEST, &e);
            }
        }
        let get = |k: &str| String::from_utf8_lossy(query.get(k.as_bytes()).unwrap()).into_owned();
        return match service
            .regions_replicated(&get("startKey"), &get("endKey"))
            .await
        {
            Ok(v) => json_response(StatusCode::OK, &v),
            Err(e) => json_response(StatusCode::BAD_REQUEST, &e),
        };
    }
    let kind = match name {
        "miss-peer" => MissPeer,
        "extra-peer" => ExtraPeer,
        "pending-peer" => PendingPeer,
        "down-peer" => DownPeer,
        "learner-peer" => LearnerPeer,
        "offline-peer" => OfflinePeer,
        "empty-region" => EmptyRegion,
        "oversized-region" => OversizedRegion,
        "undersized-region" => UndersizedRegion,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match service.region_anomalies(kind).await {
        Ok(regions) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            crate::http_region_scan::stream_regions(regions, cancel.clone()),
        )
            .into_response(),
        Err(e) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub(crate) fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [
            ("content-type", "text/plain; charset=utf-8"),
            ("x-content-type-options", "nosniff"),
        ],
        "404 page not found\n",
    )
        .into_response()
}
// Validate raw query bytes before converting the hex-only input to a Rust string.
fn validate_hex(raw: &[u8]) -> Result<(), String> {
    for &b in raw {
        if !b.is_ascii_hexdigit() {
            let mut message = format!("encoding/hex: invalid byte: U+{b:04X}");
            if (32..=126).contains(&b) || (b >= 161 && b != 173) {
                let quoted = b as char;
                message.push_str(&format!(" '{quoted}'"));
            }
            return Err(message);
        }
    }
    if raw.len() % 2 != 0 {
        return Err("encoding/hex: odd length hex string".into());
    }
    Ok(())
}
