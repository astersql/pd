// Copyright 2026 AsterSQL.
//! One-hop classic PD forwarding, preserving HTTP/2 data and trailer frames.
//! A request is sent once; a changed leader never causes a write to be replayed.
use crate::http_redirect::{RedirectState, wait_for_leader};
use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use futures_util::{Future, Stream};
use http_body_util::{BodyStream, StreamBody};
use std::{task::Poll, time::Duration};
use tonic::Status;

fn allowed(path: &str) -> bool {
    let Some(method) = path.strip_prefix("/pdpb.PD/") else {
        return false;
    };
    matches!(
        method,
        "GetMinTS"
            | "Bootstrap"
            | "IsBootstrapped"
            | "AllocID"
            | "GetStore"
            | "PutStore"
            | "GetAllStores"
            | "StoreHeartbeat"
            | "GetRegion"
            | "GetPrevRegion"
            | "GetRegionByID"
            | "ScanRegions"
            | "BatchScanRegions"
            | "AskSplit"
            | "AskBatchSplit"
            | "ReportSplit"
            | "ReportBatchSplit"
            | "GetClusterConfig"
            | "PutClusterConfig"
            | "ScatterRegion"
            | "GetOperator"
            | "SplitRegions"
            | "SplitAndScatterRegions"
            | "ReportMinResolvedTS"
            | "SetExternalTimestamp"
            | "GetExternalTimestamp"
            | "Tso"
            | "RegionHeartbeat"
            | "ReportBuckets"
            | "UpdateGCSafePoint"
            | "GetGCSafePoint"
            | "UpdateServiceGCSafePoint"
            | "GetGCSafePointV2"
            | "UpdateGCSafePointV2"
            | "UpdateServiceSafePointV2"
            | "GetAllGCSafePointV2"
            | "AdvanceGCSafePoint"
            | "AdvanceTxnSafePoint"
            | "SetGCBarrier"
            | "DeleteGCBarrier"
            | "GetGCState"
            | "GetAllKeyspacesGCStates"
            | "SetGlobalGCBarrier"
            | "DeleteGlobalGCBarrier"
    )
}
fn base(url: &str) -> Option<&str> {
    url.strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
}
fn timeout(value: &str) -> Option<Duration> {
    if !(2..=9).contains(&value.len()) {
        return None;
    }
    let (digits, unit) = value.split_at(value.len() - 1);
    if !digits.bytes().all(|v| v.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    Some(match unit {
        "H" => Duration::from_secs(n * 3600),
        "M" => Duration::from_secs(n * 60),
        "S" => Duration::from_secs(n),
        "m" => Duration::from_millis(n),
        "u" => Duration::from_micros(n),
        "n" => Duration::from_nanos(n),
        _ => return None,
    })
}
fn response(status: Status) -> Response {
    status.into_http::<Body>()
}
pub(crate) async fn middleware(
    State(state): State<RedirectState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if !allowed(request.uri().path()) {
        return next.run(request).await;
    }
    let host = request
        .headers()
        .get("pd-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if host.is_empty() {
        return next.run(request).await;
    }
    let cancel = crate::request_context::current()
        .map(|c| c.cancel)
        .unwrap_or_else(|| state.cancel.child_token());
    let deadline = request
        .headers()
        .get("grpc-timeout")
        .and_then(|v| v.to_str().ok())
        .and_then(timeout)
        .and_then(|d| tokio::time::Instant::now().checked_add(d));
    let expire = async move {
        match deadline {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending().await,
        }
    };
    let expire = Box::pin(expire);
    let work = async {
        let leader = wait_for_leader(state.services.as_ref(), true)
            .await
            .ok_or_else(|| Status::unavailable("not leader"))?;
        if !leader
            .client_urls
            .iter()
            .any(|url| base(url).is_some() && base(url) == base(&host))
        {
            return Err(Status::invalid_argument(format!(
                "forwarded host {host:?} is not a client URL of the PD leader: pd is not leader of cluster"
            )));
        }
        if leader.member_id == state.services.identity().member_id {
            return Ok(next.run(request).await);
        }
        // This host currently has no TLS material. Match Go's scheme-insensitive
        // validation; the configured transport is plaintext, independently of URL scheme.
        let uri: axum::http::Uri = host
            .parse()
            .map_err(|_| Status::invalid_argument("invalid forwarded host"))?;
        let socket = tokio::net::TcpStream::connect((
            uri.host()
                .ok_or_else(|| Status::invalid_argument("invalid forwarded host"))?
                .trim_matches(['[', ']']),
            uri.port_u16().unwrap_or(80),
        ))
        .await
        .map_err(|_| Status::unavailable("forward connection failed"))?;
        let (mut sender, connection) = hyper::client::conn::http2::handshake(
            hyper_util::rt::TokioExecutor::new(),
            hyper_util::rt::TokioIo::new(socket),
        )
        .await
        .map_err(|_| Status::unavailable("forward handshake failed"))?;
        request
            .headers_mut()
            .insert("pd-forwarded-host", "".parse().expect("empty header"));
        if let Some(at) = deadline {
            let remaining = at.saturating_duration_since(tokio::time::Instant::now());
            // Round up at the selected precision; local expiration still enforces
            // the original deadline, including time spent discovering/dialing.
            let nanos = remaining.as_nanos();
            let value = [
                (1, "n"),
                (1_000, "u"),
                (1_000_000, "m"),
                (1_000_000_000, "S"),
                (60_000_000_000, "M"),
                (3_600_000_000_000, "H"),
            ]
            .into_iter()
            .find_map(|(scale, unit)| {
                let value = nanos.div_ceil(scale);
                (value <= 99_999_999).then(|| format!("{value}{unit}"))
            })
            .expect("valid gRPC deadline fits hours");
            request
                .headers_mut()
                .insert("grpc-timeout", value.parse().expect("timeout"));
        }
        *request.uri_mut() = format!(
            "http://{}{}",
            uri.authority().expect("host"),
            request
                .uri()
                .path_and_query()
                .map(|v| v.as_str())
                .unwrap_or("/")
        )
        .parse()
        .map_err(|_| Status::invalid_argument("invalid forwarded URI"))?;
        let mut connection = Box::pin(connection);
        let upstream = tokio::select! {
            result = sender.send_request(request) => result.map_err(|_| Status::unavailable("forward request failed"))?,
            _ = &mut connection => return Err(Status::unavailable("forward connection closed")),
        };
        let (parts, body) = upstream.into_parts();
        let mut body = Box::pin(BodyStream::new(body));
        let mut driving = true;
        let mut cancelled = Box::pin(cancel.clone().cancelled_owned());
        let mut finished = false;
        let mut body_deadline = Box::pin(async move {
            match deadline {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        });
        let stream = futures_util::stream::poll_fn(move |cx| {
            if finished {
                return Poll::Ready(None);
            }
            let status = if cancelled.as_mut().poll(cx).is_ready() {
                Some(Status::cancelled("request cancelled"))
            } else if body_deadline.as_mut().poll(cx).is_ready() {
                Some(Status::deadline_exceeded("deadline exceeded"))
            } else {
                None
            };
            if let Some(status) = status {
                finished = true;
                return Poll::Ready(Some(Ok(hyper::body::Frame::trailers(
                    status.into_http::<Body>().into_parts().0.headers,
                ))));
            }
            if driving && connection.as_mut().poll(cx).is_ready() {
                driving = false;
            }
            body.as_mut().poll_next(cx)
        });
        Ok(Response::from_parts(
            parts,
            Body::new(StreamBody::new(stream)),
        ))
    };
    tokio::select! { biased;
        _ = cancel.cancelled() => response(Status::cancelled("request cancelled")),
        _ = expire => response(Status::deadline_exceeded("deadline exceeded")),
        result = work => result.unwrap_or_else(response),
    }
}
