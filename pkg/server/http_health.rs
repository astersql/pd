// Copyright 2026 AsterSQL.
//! Local probes. Missing host capabilities fail closed; no remote backend is
//! presented as an embedded consensus member.
use crate::{PdServices, http_contract::json_response, request_context};
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

/// Host-owned initialization state, independent of PD leadership.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ReadyStatus {
    pub region_loaded: bool,
    pub is_bootstrapped: bool,
}

/// Diagnostics tied to the receiving host's own consensus member. An external
/// MetaStore must not silently substitute a remote member for these checks.
#[derive(Clone, Copy, Debug)]
pub enum LocalConsensusProbe {
    SerializableRead,
    LinearizableRead,
    DataCorruption,
    NonLearner,
}
const LOCAL_CHECKS: [(&str, LocalConsensusProbe); 4] = [
    ("etcd-data-corruption", LocalConsensusProbe::DataCorruption),
    (
        "etcd-linearizable-read",
        LocalConsensusProbe::LinearizableRead,
    ),
    ("etcd-non-learner", LocalConsensusProbe::NonLearner),
    (
        "etcd-serializable-read",
        LocalConsensusProbe::SerializableRead,
    ),
];

#[derive(Clone)]
struct Probes {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
fn token(state: &Probes) -> CancellationToken {
    request_context::current()
        .map(|ctx| ctx.cancel)
        .unwrap_or_else(|| state.cancel.child_token())
}
async fn ping(State(state): State<Probes>) -> StatusCode {
    if token(&state).is_cancelled() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}
/// Probe a registered PD endpoint with a bounded, follower-safe HTTP request.
/// HTTPS is rejected until the host supplies its TLS transport; never downgrade.
pub async fn ping_url(url: &str) -> bool {
    let operation = async {
        let uri: Uri = format!("{}/pd/api/v1/ping", url.trim_end_matches('/'))
            .parse()
            .ok()?;
        // TLS is owned by the host transport work; never downgrade an HTTPS URL.
        if uri.scheme_str() != Some("http") {
            return None;
        }
        let socket = tokio::net::TcpStream::connect((
            uri.host()?.trim_matches(['[', ']']),
            uri.port_u16().unwrap_or(80),
        ))
        .await
        .ok()?;
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(socket))
                .await
                .ok()?;
        let request = Request::builder()
            .uri(uri.path_and_query()?.as_str())
            .header("host", uri.authority()?.as_str())
            .header("pd-allow-follower-handle", "true")
            .body(Body::empty())
            .ok()?;
        tokio::pin!(connection);
        tokio::select! {
            result = sender.send_request(request) => result.ok().map(|r| r.status() == StatusCode::OK),
            _ = &mut connection => None,
        }
    };
    tokio::time::timeout(Duration::from_secs(3), operation)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}
async fn health(State(state): State<Probes>) -> Response {
    let cancel = token(&state);
    let operation = async {
        let Some(store) = state.services.discovery_store() else {
            return json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                &"member discovery is not configured",
            );
        };
        let members = match crate::grpc_members::registry_members(
            store.as_ref(),
            state.services.identity().cluster_id,
        )
        .await
        {
            Ok((members, _)) => members,
            Err(error) => {
                let status = if error.code() == tonic::Code::Unavailable {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                return json_response(status, &error.message());
            }
        };
        let mut health = Vec::with_capacity(members.len());
        for member in members {
            let mut alive = false;
            for url in &member.client_urls {
                if ping_url(url).await {
                    alive = true;
                    break;
                }
            }
            health.push(serde_json::json!({"name":member.name,"member_id":member.member_id,"client_urls":member.client_urls,"health":alive}));
        }
        json_response(StatusCode::OK, &health)
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => json_response(StatusCode::SERVICE_UNAVAILABLE, &"server not started"),
        response = operation => response,
    }
}
async fn ready(
    State(state): State<Probes>,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    let cancel = token(&state);
    tokio::select! { biased;
        _ = cancel.cancelled() => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        status = state.services.probe_status() => match status {
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Ok(status) => {
                let code = if !status.is_bootstrapped || status.region_loaded { StatusCode::OK } else { StatusCode::INTERNAL_SERVER_ERROR };
                if query.iter().any(|(key, _)| key == "verbose") {
                    let mut response = json_response(code, &status);
                    response.headers_mut().insert(axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("application/json; charset=utf-8"));
                    response
                } else { code.into_response() }
            }
        }
    }
}
async fn promotion(
    State(state): State<Probes>,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    let cancel = token(&state);
    tokio::select! { biased;
        _ = cancel.cancelled() => (StatusCode::SERVICE_UNAVAILABLE, "[-]leader-promotion failed: server not started\n").into_response(),
        status = state.services.probe_status() => match status {
            Ok(status) if !status.is_bootstrapped || status.region_loaded => {
                let body = if query.iter().any(|(k,_)| k=="verbose") { "[+]leader-promotion ok\nok\n" } else { "ok\n" };
                (StatusCode::OK,body).into_response()
            }
            Ok(_) => (StatusCode::SERVICE_UNAVAILABLE,"[-]leader-promotion failed: regions not loaded\n").into_response(),
            Err(_) => (StatusCode::SERVICE_UNAVAILABLE,"[-]leader-promotion failed: server not started\n").into_response(),
        }
    }
}
async fn checks(
    State(state): State<Probes>,
    uri: Uri,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    let path = uri.path();
    let live = path == "/pd/api/v2/livez" || path.starts_with("/pd/api/v2/livez/");
    let base = if live {
        "/pd/api/v2/livez"
    } else {
        "/pd/api/v2/readyz"
    };
    let individual = path.strip_prefix(&format!("{base}/"));
    let mut names: Vec<_> = if live {
        vec!["etcd-serializable-read"]
    } else {
        LOCAL_CHECKS
            .iter()
            .map(|(name, _)| *name)
            .chain(std::iter::once("leader-promotion"))
            .collect()
    };
    if let Some(name) = individual {
        if !names.contains(&name) {
            return (
                StatusCode::NOT_FOUND,
                [("content-type", "text/plain")],
                "404 page not found",
            )
                .into_response();
        }
        names.retain(|n| *n == name);
    } else {
        names.retain(|name| {
            !query
                .iter()
                .any(|(key, value)| key == "exclude" && value.as_str() == *name)
        });
    }
    let cancel = token(&state);
    let operation = async {
        if !state.services.probe_listener_ready() {
            return (StatusCode::SERVICE_UNAVAILABLE, "server not started\n").into_response();
        }
        let mut failed = false;
        let mut output = String::new();
        for name in names {
            let result = if name == "leader-promotion" {
                match state.services.probe_status().await {
                    Ok(status) if !status.is_bootstrapped || status.region_loaded => Ok(()),
                    Ok(_) => Err("regions not loaded".into()),
                    Err(_) => Err("server not started".into()),
                }
            } else {
                let probe = LOCAL_CHECKS
                    .iter()
                    .find(|(key, _)| *key == name)
                    .expect("validated check")
                    .1;
                state.services.local_consensus_probe(probe).await
            };
            match result {
                Ok(()) => output.push_str(&format!("[+]{name} ok\n")),
                Err(error) => {
                    failed = true;
                    output.push_str(&format!("[-]{name} failed: {error}\n"));
                }
            }
        }
        if failed {
            (StatusCode::SERVICE_UNAVAILABLE, output).into_response()
        } else {
            if !query.iter().any(|(key, _)| key == "verbose") {
                output.clear();
            }
            output.push_str("ok\n");
            (StatusCode::OK, output).into_response()
        }
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => (StatusCode::SERVICE_UNAVAILABLE,"server not started\n").into_response(),
        response = operation => response,
    }
}

pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/health", get(health))
        .route("/pd/health", get(health))
        .route("/pd/api/v1/ping", get(ping))
        .route("/pd/ping", get(ping))
        .route("/pd/api/v2/ready", get(ready))
        .route("/pd/api/v2/readyz/leader-promotion", get(promotion))
        .route("/pd/api/v2/livez", get(checks))
        .route("/pd/api/v2/livez/{check}", get(checks))
        .route("/pd/api/v2/readyz", get(checks))
        .route("/pd/api/v2/readyz/{check}", get(checks))
        .with_state(Probes { services, cancel })
}
