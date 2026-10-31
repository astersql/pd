// Copyright 2026 AsterSQL.
//! Request-scoped diagnostics. Client-supplied fields are never authentication
//! identities or authorization inputs. No global runtime or observer is installed.
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use futures_util::StreamExt;
use http_body_util::{BodyStream, StreamBody};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

/// Metadata available while the adapter or its response stream is being polled.
#[derive(Clone, Debug)]
pub struct RequestContext {
    /// Opaque client correlation ID, or a router-local generated value.
    pub request_id: String,
    /// Registered HTTP handler name or full gRPC method; never a client label.
    pub service_label: String,
    /// HTTP caller/component header; diagnostic only, not protobuf CallerId.
    pub caller_id: String,
    pub source_ip: String,
    pub source_port: String,
    /// All forwarded-for fields, retained in wire order for diagnostics only.
    pub forwarded_for: Vec<String>,
    /// gRPC forwarding target metadata; never used to establish peer identity.
    pub forwarded_host: String,
    /// Transport peer supplied by the listener, independent of forwarded headers.
    pub peer: Option<SocketAddr>,
    /// Cancels when the request/body is dropped, completes, or the host stops.
    /// A host that spawns work must explicitly pass this token and join that work.
    pub cancel: CancellationToken,
}

tokio::task_local! { static CONTEXT: RequestContext; }

/// Context of the currently polled request, absent outside the transport scope.
/// Tokio does not implicitly inherit this into spawned tasks.
pub fn current() -> Option<RequestContext> {
    CONTEXT.try_with(Clone::clone).ok()
}

#[derive(Clone)]
pub(crate) struct ContextState {
    cancel: CancellationToken,
    sequence: Arc<AtomicU64>,
    prefix: String,
}
impl ContextState {
    pub(crate) fn new(cancel: CancellationToken) -> Self {
        Self {
            cancel,
            sequence: Arc::new(AtomicU64::new(1)),
            prefix: format!(
                "{:x}-{:x}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
        }
    }
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

/// Go GetIPPortFromHTTPRequest precedence, including its deliberately permissive
/// treatment of proxy text and host:port overriding X-Forwarded-Port.
pub(crate) fn http_source(headers: &HeaderMap, peer: Option<SocketAddr>) -> (String, String) {
    let forwarded = header(headers, "x-forwarded-for")
        .split(',')
        .next()
        .unwrap_or_default()
        .trim_matches(' ');
    let (source, port) = if !forwarded.is_empty() {
        (
            forwarded.to_owned(),
            header(headers, "x-forwarded-port")
                .split(',')
                .next()
                .unwrap_or_default()
                .trim_matches(' ')
                .to_owned(),
        )
    } else if !header(headers, "x-real-ip").is_empty() {
        (header(headers, "x-real-ip").to_owned(), String::new())
    } else {
        (
            peer.map(|p| p.to_string()).unwrap_or_default(),
            String::new(),
        )
    };
    // net.SplitHostPort accepts nonnumeric ports and bracketed arbitrary hosts.
    let split = if let Some(rest) = source.strip_prefix('[') {
        rest.split_once("]:")
            .filter(|(host, port)| !host.contains(['[', ']']) && !port.contains([':', '[', ']']))
    } else {
        source
            .split_once(':')
            .filter(|(host, port)| !host.contains(['[', ']']) && !port.contains([':', '[', ']']))
    };
    split
        .map(|(ip, port)| (ip.to_owned(), port.to_owned()))
        .unwrap_or((source, port))
}
pub(crate) fn caller(headers: &HeaderMap) -> String {
    ["x-caller-id", "component"]
        .into_iter()
        .map(|name| header(headers, name))
        .find(|v| !v.is_empty())
        .unwrap_or("anonymous")
        .to_owned()
}

// Own cancellation across the response future AND every data/trailer frame.
struct Lifetime(RequestContext);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.cancel.cancel();
    }
}

pub(crate) async fn middleware(
    State(state): State<ContextState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|v| v.0)
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<crate::service_auth::PeerAddr>>()
                .map(|v| v.0.0)
        });
    let path = request.uri().path();
    let grpc = path.starts_with("/pdpb.PD/");
    let service_label = if grpc {
        path.to_owned()
    } else {
        crate::http_service_limit::service_label(request.method().as_str(), path).to_owned()
    };
    let headers = request.headers();
    let supplied = header(headers, "x-request-id");
    let request_id = if !supplied.is_empty()
        && supplied.len() <= 128
        && supplied.bytes().all(|b| b.is_ascii_graphic())
    {
        supplied.to_owned()
    } else {
        format!(
            "{}-{}",
            state.prefix,
            state.sequence.fetch_add(1, Ordering::Relaxed)
        )
    };
    let (source_ip, source_port) = if grpc {
        peer.map(|p| (p.ip().to_string(), p.port().to_string()))
            .unwrap_or_default()
    } else {
        http_source(headers, peer)
    };
    let context = RequestContext {
        request_id,
        service_label,
        caller_id: if grpc { String::new() } else { caller(headers) },
        source_ip,
        source_port,
        forwarded_for: headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(|v| v.trim_matches(' ').to_owned())
            .collect(),
        forwarded_host: if grpc {
            header(headers, "pd-forwarded-host").to_owned()
        } else {
            String::new()
        },
        peer,
        cancel: state.cancel.child_token(),
    };
    request.extensions_mut().insert(context.clone());
    let lifetime = Lifetime(context.clone());
    let response = CONTEXT.scope(context, next.run(request)).await;
    let (parts, body) = response.into_parts();
    let stream = futures_util::stream::unfold(
        (BodyStream::new(body), lifetime),
        |(mut stream, lifetime)| async move {
            let frame = CONTEXT.scope(lifetime.0.clone(), stream.next()).await?;
            Some((frame, (stream, lifetime)))
        },
    );
    Response::from_parts(parts, Body::new(StreamBody::new(stream)))
}
