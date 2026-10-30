// Copyright 2026 AsterSQL.
//! Region point reads over the host cache and canonical response encoders.
use crate::{
    PdServices,
    http_contract::{HttpError, json_response, parse_uint64_field, query_unescape},
    request_context,
};
use axum::{
    Router,
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
    Router::new()
        .route("/pd/api/v1/region/id/{id}", get(handle))
        .route("/pd/api/v1/region/id/{id}/label/{key}", get(handle))
        .route("/pd/api/v1/region/key/{key}", get(handle))
        .route("/pd/api/v1/regions/sibling/{id}", get(handle))
        .with_state(Context { services, cancel })
}
fn error(code: StatusCode, message: impl AsRef<str>) -> Response {
    json_response(code, &message.as_ref())
}
fn input(e: HttpError) -> Response {
    match e {
        HttpError::Input(e) => error(StatusCode::BAD_REQUEST, e),
        _ => error(StatusCode::BAD_REQUEST, "invalid input"),
    }
}
async fn handle(State(s): State<Context>, OriginalUri(uri): OriginalUri) -> Response {
    // Gorilla's parent router cleans the decoded URL path before dispatch,
    // even for the nested UseEncodedPath key route.
    if let Ok(decoded) = query_unescape(&uri.path().replace('+', "%2B")) {
        let mut parts: Vec<&[u8]> = Vec::new();
        for part in decoded.split(|b| *b == b'/') {
            match part {
                b"" | b"." => {}
                b".." => {
                    parts.pop();
                }
                _ => parts.push(part),
            }
        }
        let mut clean = vec![b'/'];
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                clean.push(b'/')
            }
            clean.extend_from_slice(part)
        }
        if decoded.ends_with(b"/") && clean.len() > 1 {
            clean.push(b'/')
        }
        if clean != decoded {
            let mut location = String::new();
            for b in clean {
                if b.is_ascii_alphanumeric() || b"/-_.~:@&=+$".contains(&b) {
                    location.push(b as char)
                } else {
                    location.push_str(&format!("%{b:02X}"))
                }
            }
            if let Some(q) = uri.query() {
                location.push('?');
                location.push_str(q)
            }
            return (
                StatusCode::MOVED_PERMANENTLY,
                [("location", location)],
                axum::body::Body::empty(),
            )
                .into_response();
        }
    }
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let label = uri.path().contains("/label/");
        let follower = s.services.healthy().await.is_err_and(|e| e == "not leader");
        if !follower || label {
            match s.services.is_bootstrapped().await {
                Ok(true) => {}
                Ok(false) => {
                    return error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                    );
                }
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            }
        }
        let Some(service) = (if follower && !label {
            s.services.follower_read_service()
        } else {
            s.services.cluster_read_service()
        }) else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "cluster reads are not configured",
            );
        };
        if follower && !label {
            match service.query_regions(&[], &[], &[], false).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first",
                    );
                }
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            }
        }
        let path = uri.path();
        let key_lookup = path.starts_with("/pd/api/v1/region/key/");
        let sibling = path.starts_with("/pd/api/v1/regions/sibling/");
        let raw = if label {
            path.trim_start_matches("/pd/api/v1/region/id/")
                .split('/')
                .next()
                .unwrap_or_default()
        } else {
            path.rsplit('/').next().unwrap_or_default()
        };
        let mut id = 0;
        let region = if key_lookup {
            let mut key = match query_unescape(raw) {
                Ok(k) => k,
                Err(e) => return input(e),
            };
            let format = uri
                .query()
                .unwrap_or_default()
                .split('&')
                .filter(|s| !s.contains(';'))
                .find_map(|p| {
                    let (k, v) = p.split_once('=').unwrap_or((p, ""));
                    (query_unescape(k).ok()?.as_slice() == b"format")
                        .then(|| query_unescape(v).ok())
                        .flatten()
                });
            if format.as_deref() == Some(b"hex") {
                let mut decoded = Vec::new();
                for chunk in key.chunks(2) {
                    let mut byte = 0;
                    for c in chunk {
                        let Some(n) = (*c as char).to_digit(16) else {
                            return error(
                                StatusCode::BAD_REQUEST,
                                if (*c as char).is_control() {
                                    format!("encoding/hex: invalid byte: U+{c:04X}")
                                } else {
                                    format!(
                                        "encoding/hex: invalid byte: U+{:04X} '{}'",
                                        c, *c as char
                                    )
                                },
                            );
                        };
                        byte = byte * 16 + n as u8;
                    }
                    if chunk.len() != 2 {
                        return error(
                            StatusCode::BAD_REQUEST,
                            "encoding/hex: odd length hex string",
                        );
                    }
                    decoded.push(byte);
                }
                key = decoded;
            }
            service.region_by_key(&key, false).await
        } else {
            let raw = match query_unescape(&raw.replace('+', "%2B")) {
                Ok(v) => String::from_utf8_lossy(&v).into_owned(),
                Err(e) => return input(e),
            };
            id = if sibling {
                match raw.parse::<i64>() {
                    Ok(v) => v as u64,
                    Err(_) => {
                        let digits = raw.strip_prefix(['+', '-']).unwrap_or(&raw);
                        let reason =
                            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                                "value out of range"
                            } else {
                                "invalid syntax"
                            };
                        return error(
                            StatusCode::BAD_REQUEST,
                            format!("strconv.Atoi: parsing {raw:?}: {reason}"),
                        );
                    }
                }
            } else {
                match parse_uint64_field(Some(&raw), "id") {
                    Ok(v) => v,
                    Err(e) => return input(e),
                }
            };
            if id == 0 && !sibling && !label {
                return json_response(StatusCode::BAD_REQUEST, &serde_json::json!({}));
            }
            service.region(id).await
        };
        let region = match region {
            Ok(Some(r)) => r,
            Ok(None) if label => {
                return json_response(StatusCode::NOT_FOUND, &serde_json::Value::Null);
            }
            Ok(None) => {
                return error(
                    StatusCode::NOT_FOUND,
                    format!(
                        "[PD:region:ErrRegionNotFound]region {} not found",
                        if key_lookup {
                            "%v".into()
                        } else {
                            id.to_string()
                        }
                    ),
                );
            }
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
        };
        if label {
            let key = match query_unescape(
                &path
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .replace('+', "%2B"),
            ) {
                Ok(v) => String::from_utf8_lossy(&v).into_owned(),
                Err(e) => return input(e),
            };
            return match s.services.region_label(region, key).await {
                Ok(v) => json_response(StatusCode::OK, &v),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
        }
        if sibling {
            let (left, right) = match service.adjacent_regions(&region).await {
                Ok(v) => v,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e),
            };
            let bytes = astersql_pd_response::marshal_regions_info_json(
                &[left.as_deref(), right.as_deref()],
                || {
                    token
                        .is_cancelled()
                        .then_some(astersql_pd_response::RegionListCancellation::Cancelled)
                },
            );
            return match bytes {
                Ok(bytes) => (
                    StatusCode::OK,
                    [("content-type", "application/octet-stream")],
                    bytes,
                )
                    .into_response(),
                Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            };
        }
        (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            serde_json::to_vec(&astersql_pd_response::new_api_region_info(Some(&region)))
                .expect("region JSON"),
        )
            .into_response()
    };
    tokio::select! {biased; _=token.cancelled()=>error(StatusCode::INTERNAL_SERVER_ERROR,"context canceled"), r=work=>r}
}
