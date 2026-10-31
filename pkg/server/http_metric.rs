// Copyright 2026 AsterSQL.
//! Classic metric proxy; upstream addresses are validated and pinned before dial.
use crate::{
    PdServices,
    metric_transport::{self, Target},
    request_context,
};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Query {
    services: Arc<dyn PdServices>,
    cancel: CancellationToken,
}
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v1/metric/query", get(query).post(query))
        .route("/pd/api/v1/metric/query_range", get(query).post(query))
        .with_state(Query { services, cancel })
}
fn error() -> Response {
    (
        StatusCode::BAD_GATEWAY,
        [
            ("content-type", "application/json; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        r#"{"status":"error","errorType":"proxy","error":"metric query failed"}"#,
    )
        .into_response()
}
async fn query(State(state): State<Query>, mut request: Request) -> Response {
    let cancel = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| state.cancel.child_token());
    let work = async {
        let config = match state.services.telemetry_config() {
            Some(c) => c,
            None => Arc::new(state.services.configuration(false).await?),
        };
        let target = Target::parse(&config.pd_server.metric_storage)?;
        let path = request
            .uri()
            .path_and_query()
            .ok_or("missing query path")?
            .as_str()
            .replacen("/pd/api/v1/metric", "/api/v1", 1);
        *request.uri_mut() = path.parse().map_err(|_| "invalid query path")?;
        *request.version_mut() = axum::http::Version::HTTP_11;
        metric_transport::strip_hop(request.headers_mut());
        request.headers_mut().remove("accept-encoding");
        // Go's cloned HTTP transport negotiates gzip after Rewrite removes the
        // client preference, except for Range requests.
        if !request.headers().contains_key("range") {
            request
                .headers_mut()
                .insert("accept-encoding", "gzip".parse().unwrap());
        }

        request
            .headers_mut()
            .insert("accept", "application/json".parse().unwrap());
        request.headers_mut().insert(
            "host",
            target
                .uri
                .authority()
                .ok_or("missing authority")?
                .as_str()
                .parse()
                .map_err(|_| "invalid authority")?,
        );
        let mut response = metric_transport::send(
            target,
            request.map(|body| axum::body::Body::from_stream(body.into_data_stream())),
            &config,
            true,
            state
                .services
                .service_metrics()
                .and_then(|m| m.client_tls()),
            cancel.clone(),
            tokio::time::Instant::now() + Duration::from_secs(30),
        )
        .await?;
        if !response.status().is_success() {
            return Err("upstream status".to_string());
        }
        if response
            .headers()
            .get("content-encoding")
            .is_some_and(|v| v == "gzip")
        {
            let (parts, body) = response.into_parts();
            response = Response::from_parts(parts, metric_transport::gunzip(body));
        }
        *response.headers_mut() = axum::http::HeaderMap::new();
        response.headers_mut().insert(
            "content-type",
            "application/json; charset=utf-8".parse().unwrap(),
        );
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        Ok::<_, String>(response)
    };
    tokio::select! {biased;_=cancel.cancelled()=>error(), result=work=>result.unwrap_or_else(|_|error())}
}
