// Copyright 2026 AsterSQL.
//! Classic PD HTTP forwarding. The connection driver lives in the request/body
//! future, so dropping either closes the upstream without detached workers.
use crate::{PdServices, request_context};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, Read};
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures_util::{Future, Stream, StreamExt};
use http_body_util::{BodyStream, StreamBody};
use prost::Message;
use std::io::Write;
use std::{sync::Arc, task::Poll, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(crate) struct RedirectState {
    pub services: Arc<dyn PdServices>,
    pub cancel: CancellationToken,
}
fn error(v2: bool, status: StatusCode, message: &str) -> Response {
    if v2 {
        (
            status,
            [("content-type", "application/json; charset=utf-8")],
            serde_json::to_vec(message).expect("string JSON"),
        )
            .into_response()
    } else {
        (
            status,
            [
                ("content-type", "text/plain; charset=utf-8"),
                ("x-content-type-options", "nosniff"),
            ],
            format!("{message}\n"),
        )
            .into_response()
    }
}
fn local(request: &Request<Body>) -> bool {
    let path = request.uri().path();
    (request.method() == "GET" && path == "/pd/api/v1/config")
        || [
            "/pd/api/v1/admin/log",
            "/pd/api/v1/status",
            "/pd/api/v1/health",
            "/pd/api/v1/ping",
            "/pd/api/v1/version",
            "/pd/api/v1/debug/pprof",
        ]
        .iter()
        .any(|p| path.starts_with(p))
}
fn nonempty(request: &Request<Body>, name: &str) -> bool {
    request
        .headers()
        .get(name)
        .is_some_and(|v| !v.as_bytes().is_empty())
}
async fn leader(services: &dyn PdServices) -> Option<pdpb::Member> {
    let store = services.discovery_store()?;
    let key = format!("/pd/{}/leader", services.identity().cluster_id);
    let result = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .ok()?;
    pdpb::Member::decode(result.entries.first()?.value.as_slice()).ok()
}
pub(crate) async fn wait_for_leader(services: &dyn PdServices, v2: bool) -> Option<pdpb::Member> {
    let mut target = leader(services).await;
    if !v2 {
        let mut delay = 100;
        let mut elapsed = 0;
        while target.is_none() && elapsed < 3000 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            elapsed += delay;
            if elapsed >= 3000 {
                break;
            }
            target = leader(services).await;
            delay = (delay * 2).min(3000 - elapsed);
        }
    }
    target
}
pub(crate) async fn middleware(
    State(state): State<RedirectState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if !request.uri().path().starts_with("/pd/") || local(&request) {
        return next.run(request).await;
    }
    let v2 = request.uri().path().starts_with("/pd/api/v2/");
    let token = request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| state.cancel.child_token());
    tokio::select! { biased;
        _ = token.cancelled() => error(v2, StatusCode::INTERNAL_SERVER_ERROR, "[PD:server:ErrServerNotStarted]server not started"),
        response = forward(state.services, request, next, v2, token.clone()) => response,
    }
}
async fn forward(
    services: Arc<dyn PdServices>,
    mut request: Request<Body>,
    next: Next,
    v2: bool,
    cancel: CancellationToken,
) -> Response {
    if nonempty(&request, "pd-allow-follower-handle") || services.healthy().await.is_ok() {
        return next.run(request).await;
    }
    if nonempty(&request, "pd-redirector") {
        return error(
            v2,
            StatusCode::INTERNAL_SERVER_ERROR,
            "[PD:apiutil:ErrRedirectToNotLeader]redirect to not leader",
        );
    }
    let target = wait_for_leader(services.as_ref(), v2).await;
    let Some(target) = target else {
        return error(
            v2,
            StatusCode::SERVICE_UNAVAILABLE,
            if v2 {
                "[PD:server:ErrLeaderNil]leader is nil"
            } else {
                "[PD:apiutil:ErrRedirectNoLeader]redirect finds no leader"
            },
        );
    };
    if !v2 && (target.name == services.identity().name || services.healthy().await.is_ok()) {
        return next.run(request).await;
    }
    let Ok(name) = services.identity().name.parse() else {
        return error(
            v2,
            StatusCode::INTERNAL_SERVER_ERROR,
            "[PD:apiutil:ErrRedirect]redirect failed",
        );
    };
    request.headers_mut().insert("pd-redirector", name);
    if !v2 && let Some(context) = request_context::current() {
        if let Ok(value) = context.source_ip.parse() {
            request.headers_mut().append("x-forwarded-for", value);
        }
        if !context.source_port.is_empty()
            && let Ok(value) = context.source_port.parse()
        {
            request.headers_mut().append("x-forwarded-port", value);
        }
    }
    // Dial fallback is safe before the streaming request body is consumed. Once
    // sent, do not replay a potentially committed write to another endpoint.
    for url in target.client_urls {
        let Ok(uri) = url.parse::<Uri>() else {
            continue;
        };
        if uri.scheme_str() != Some("http") {
            continue;
        }
        let Some(host) = uri.host() else {
            continue;
        };
        let Ok(socket) = tokio::net::TcpStream::connect((
            host.trim_matches(['[', ']']),
            uri.port_u16().unwrap_or(80),
        ))
        .await
        else {
            continue;
        };
        let Ok((mut sender, connection)) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(socket)).await
        else {
            continue;
        };
        // Origin-form retains the exact escaped path/query. Preserve incoming
        // Host just as Go Client.Do does when Request.Host is already set.
        if !request.headers().contains_key("host")
            && let Some(authority) = uri.authority()
        {
            request
                .headers_mut()
                .insert("host", authority.as_str().parse().expect("URI authority"));
        }
        let path = request
            .uri()
            .path_and_query()
            .map(|v| v.as_str())
            .unwrap_or("/")
            .to_owned();
        *request.uri_mut() = path.parse().expect("existing path and query");
        *request.version_mut() = axum::http::Version::HTTP_11;
        let mut connection = Box::pin(connection);
        let response = tokio::select! {
            result = sender.send_request(request) => result,
            _ = &mut connection => return error(false, StatusCode::INTERNAL_SERVER_ERROR, "[PD:apiutil:ErrRedirect]redirect failed"),
        };
        let Ok(response) = response else {
            break;
        };
        let (mut parts, body) = response.into_parts();
        parts.headers.remove("content-length");
        let gzip = parts
            .headers
            .get("content-encoding")
            .is_some_and(|v| v == "gzip");
        parts.headers.remove("content-encoding");
        let mut body = Box::pin(BodyStream::new(body));
        let mut driving = true;
        let mut cancelled = Box::pin(cancel.cancelled_owned());
        let stream = futures_util::stream::poll_fn(move |cx| {
            if cancelled.as_mut().poll(cx).is_ready() {
                return Poll::Ready(None);
            }
            if driving && connection.as_mut().poll(cx).is_ready() {
                driving = false;
            }
            body.as_mut().poll_next(cx)
        });
        if gzip {
            let mut stream = Box::pin(stream);
            let mut decoder = flate2::write::GzDecoder::new(Vec::new());
            // Go rejects invalid gzip headers before committing response status.
            while decoder.header().is_none() {
                match stream.next().await {
                    Some(Ok(frame)) => {
                        if let Ok(data) = frame.into_data() {
                            if decoder.write_all(&data).is_err() {
                                return error(
                                    false,
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    "[PD:apiutil:ErrRedirect]redirect failed",
                                );
                            }
                        }
                    }
                    _ => {
                        return error(
                            false,
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "[PD:apiutil:ErrRedirect]redirect failed",
                        );
                    }
                }
            }
            let mut initial = Some(std::mem::take(decoder.get_mut()));
            let mut finished = false;
            let decoded = futures_util::stream::poll_fn(move |cx| {
                if let Some(data) = initial.take().filter(|v| !v.is_empty()) {
                    return Poll::Ready(Some(Ok::<_, std::io::Error>(axum::body::Bytes::from(
                        data,
                    ))));
                }
                if finished {
                    return Poll::Ready(None);
                }
                loop {
                    match stream.as_mut().poll_next(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(None) => {
                            finished = true;
                            if let Err(e) = decoder.try_finish() {
                                return Poll::Ready(Some(Err(e)));
                            }
                            let data = std::mem::take(decoder.get_mut());
                            return Poll::Ready(if data.is_empty() {
                                None
                            } else {
                                Some(Ok(axum::body::Bytes::from(data)))
                            });
                        }
                        Poll::Ready(Some(Err(e))) => {
                            finished = true;
                            return Poll::Ready(Some(Err(std::io::Error::other(e))));
                        }
                        Poll::Ready(Some(Ok(frame))) => {
                            if let Ok(data) = frame.into_data() {
                                if let Err(e) = decoder.write_all(&data) {
                                    finished = true;
                                    return Poll::Ready(Some(Err(e)));
                                }
                                let data = std::mem::take(decoder.get_mut());
                                if !data.is_empty() {
                                    return Poll::Ready(Some(Ok(axum::body::Bytes::from(data))));
                                }
                            }
                        }
                    }
                }
            });
            return Response::from_parts(parts, Body::from_stream(decoded));
        }
        return Response::from_parts(parts, Body::new(StreamBody::new(stream)));
    }
    error(
        false,
        StatusCode::INTERNAL_SERVER_ERROR,
        "[PD:apiutil:ErrRedirect]redirect failed",
    )
}
