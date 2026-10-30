// Copyright 2026 AsterSQL.
//! Read-only projections of the host's canonical configuration.
use crate::{PdServices, http_contract::json_response, request_context};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{Request, StatusCode, Uri},
    response::Response,
    routing::get,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ConfigRead {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}

async fn leader_config(s: &ConfigRead) -> Result<serde_json::Value, String> {
    let target = crate::http_redirect::leader(s.services.as_ref())
        .await
        .ok_or("[PD:server:ErrLeaderNil]leader is nil")?;
    let url = target
        .client_urls
        .first()
        .ok_or("[PD:server:ErrLeaderNil]leader is nil")?;
    // A local follower must not recursively fetch itself during role transitions.
    if target.member_id == s.services.identity().member_id {
        return Err("[PD:server:ErrLeaderNil]leader is nil".into());
    }
    let uri: Uri = url
        .parse()
        .map_err(|e: axum::http::uri::InvalidUri| e.to_string())?;
    if uri.scheme_str() != Some("http") {
        return Err("leader HTTP transport is not configured".into());
    }
    let socket = tokio::net::TcpStream::connect((
        uri.host()
            .ok_or("missing leader host")?
            .trim_matches(['[', ']']),
        uri.port_u16().unwrap_or(80),
    ))
    .await
    .map_err(|e| e.to_string())?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(socket))
            .await
            .map_err(|e| e.to_string())?;
    let work = async {
        let response = sender
            .send_request(
                Request::builder()
                    .uri("/pd/api/v1/config")
                    .header(
                        "host",
                        uri.authority().ok_or("missing leader authority")?.as_str(),
                    )
                    .body(Body::empty())
                    .map_err(|e| e.to_string())?,
            )
            .await
            .map_err(|e| e.to_string())?;
        if response.status() != StatusCode::OK {
            return Err("[PD:http:ErrSendRequest]send HTTP request failed".into());
        }
        let body = response
            .into_body()
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        serde_json::from_slice(&body).map_err(|e| e.to_string())
    };
    tokio::pin!(connection);
    tokio::select! { result = work => result, result = &mut connection => Err(format!("leader connection closed: {result:?}")) }
}
async fn read(State(s): State<ConfigRead>, OriginalUri(uri): OriginalUri) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| s.cancel.child_token());
    let work = async {
        let path = uri.path().strip_prefix("/pd/api/v1/config").unwrap_or("");
        let defaults = path == "/default";
        let mut config = s.services.configuration(defaults).await?;
        if !defaults && config.schedule.max_merge_region_keys == 0 {
            config.schedule.max_merge_region_keys =
                config.schedule.max_merge_region_size.wrapping_mul(10_000);
        }
        let mut value = serde_json::to_value(config).map_err(|e| e.to_string())?;
        if path.is_empty() && s.services.healthy().await.is_err() {
            let leader = leader_config(&s).await?;
            for key in ["schedule", "replication"] {
                value[key] = leader
                    .get(key)
                    .ok_or("missing leader configuration section")?
                    .clone();
            }
        }
        Ok::<_, String>(match path {
            "" | "/default" => value,
            "/replicate" => value["replication"].take(),
            section => value[&section[1..]].take(),
        })
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::INTERNAL_SERVER_ERROR, &"[PD:server:ErrServerNotStarted]server not started"),
        result = work => match result { Ok(value) => json_response(StatusCode::OK, &value), Err(error) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &error) }
    }
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    let mut router = Router::new();
    for suffix in [
        "",
        "/default",
        "/schedule",
        "/pd-server",
        "/replicate",
        "/label-property",
        "/cluster-version",
        "/replication-mode",
    ] {
        router = router.route(&format!("/pd/api/v1/config{suffix}"), get(read));
    }
    router.with_state(ConfigRead { services, cancel })
}
