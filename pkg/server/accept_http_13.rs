// Copyright 2026 AsterSQL.
//! HTTP conversions and registered-target forwarding over canonical MCS discovery.
use crate::PdServices;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_mcs_discovery::get_ms_members;
use astersql_pd_storage_domain::{Consistency, Read};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, State},
    http::{Method, Request, StatusCode},
    response::Response,
    routing::{get, post},
};
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
pub(crate) fn router(s: Arc<dyn PdServices>, c: CancellationToken) -> Router {
    Router::new()
        .route("/pd/api/v2/ms/members/{service}", get(handle))
        .route("/pd/api/v2/ms/primary/{service}", get(handle))
        .route("/pd/api/v2/ms/tso/primary/evict", post(handle))
        .route(
            "/pd/api/v2/ms/members/{service}/",
            get(crate::accept_http_09::redirect),
        )
        .route(
            "/pd/api/v2/ms/primary/{service}/",
            get(crate::accept_http_09::redirect),
        )
        .route(
            "/pd/api/v2/ms/tso/primary/evict/",
            post(crate::accept_http_09::redirect),
        )
        .with_state((s, c))
}
fn json(status: u16, v: serde_json::Value) -> Response {
    let mut response =
        crate::http_contract::json_response(StatusCode::from_u16(status).unwrap(), &v);
    response.headers_mut().insert(
        "content-type",
        "application/json; charset=utf-8".parse().unwrap(),
    );
    response
}
async fn handle(
    State((s, c)): State<(Arc<dyn PdServices>, CancellationToken)>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response {
    if !s.mcs_enabled() {
        return json(404, serde_json::json!("not support microservice"));
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or(c);
    let work = async {
        let Some(store) = s.discovery_store() else {
            return json(500, serde_json::json!("server not started"));
        };
        let id = s.identity().cluster_id;
        let path = uri.path();
        if path.contains("/members/") {
            let raw = path.rsplit('/').next().unwrap();
            let decoded = crate::http_contract::query_unescape(&raw.replace('+', "%2B"))
                .unwrap_or_else(|_| raw.as_bytes().to_vec());
            let service = String::from_utf8_lossy(&decoded);
            return match get_ms_members(store.as_ref(), id, &service, &cancel).await {
                Ok(m) => json(200, serde_json::json!(m)),
                Err(e) => json(500, serde_json::json!(e.to_string())),
            };
        }
        if path.contains("/primary/") && request.method() == Method::GET {
            let raw = path.rsplit('/').next().unwrap();
            let decoded = crate::http_contract::query_unescape(&raw.replace('+', "%2B"))
                .unwrap_or_else(|_| raw.as_bytes().to_vec());
            let service = String::from_utf8_lossy(&decoded);
            let key = ClusterPaths::new(id).election_path(Some(&MsParam {
                service_name: service.into(),
                group_id: 0,
            }));
            let value = match store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
            {
                Ok(s) => s.entries.into_iter().next(),
                Err(_) => None,
            };
            let address = value
                .and_then(|e| {
                    astersql_pd_proto::tsopb::Participant::decode(e.value.as_slice()).ok()
                })
                .and_then(|p| p.listen_urls.into_iter().next())
                .unwrap_or_default();
            return json(200, serde_json::json!(address));
        }
        let node = crate::accept_http_09::query(uri.query().unwrap_or(""))
            .remove("node")
            .unwrap_or_default();
        if node.is_empty() {
            return json(400, serde_json::json!("please specify the tso node"));
        }
        let entries = match get_ms_members(store.as_ref(), id, "tso", &cancel).await {
            Ok(e) => e.unwrap_or_default(),
            Err(e) => return json(500, serde_json::json!(e.to_string())),
        };
        let Some(entry) = entries
            .iter()
            .find(|e| astersql_pd_typeutil::equal_base_urls(&e.service_addr, &node))
        else {
            return json(400, serde_json::json!(format!("{node} is not a tso node")));
        };
        proxy(&entry.service_addr, request, &cancel, s.telemetry_config())
            .await
            .unwrap_or_else(|| {
                use axum::response::IntoResponse;
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [
                        ("content-type", "text/plain; charset=utf-8"),
                        ("x-content-type-options", "nosniff"),
                    ],
                    "[PD:apiutil:ErrRedirect]redirect failed\n",
                )
                    .into_response()
            })
    };
    tokio::select! {biased;_=cancel.cancelled()=>json(503,serde_json::json!("context canceled")),r=work=>r}
}
/// Stream one request once to the canonical registered address; never replay a write.
async fn proxy(
    target: &str,
    mut request: Request<Body>,
    cancel: &CancellationToken,
    config: Option<Arc<astersql_pd_server_config::Config>>,
) -> Option<Response> {
    let config = config.unwrap_or_else(|| Arc::new(Default::default()));
    let scheme = if config.security.tls.cert_path.is_empty() {
        "http"
    } else {
        "https"
    };
    let target = crate::metric_transport::Target::parse(&if target.contains("://") {
        target.to_owned()
    } else {
        format!("{scheme}://{target}")
    })
    .ok()?;
    *request.uri_mut() = "/tso/api/v1/primary/evict".parse().ok()?;
    request
        .headers_mut()
        .insert("host", target.uri.authority()?.as_str().parse().ok()?);
    crate::metric_transport::send(
        target,
        request,
        &config,
        false,
        None,
        cancel.clone(),
        tokio::time::Instant::now() + Duration::from_secs(30),
    )
    .await
    .ok()
}
