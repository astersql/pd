// Copyright 2026 AsterSQL.
//! Request-owned HTTP transport shared by metrics and MCS control requests.
use axum::{
    body::{Body, Bytes},
    http::{Request, Response, Uri},
};
use futures_util::{Future, Stream};
use http_body_util::BodyStream;
use rustls::pki_types::ServerName;
use std::{net::SocketAddr, sync::Arc, task::Poll, time::Duration};
use tokio_util::sync::CancellationToken;
trait Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Io for T {}
/// Drive one request without spawned tasks. The caller resolves and validates
/// addresses, supplies TLS policy, and owns cancellation and the total deadline.
/// The returned body owns its connection; dropping it releases transport resources.
pub async fn send(
    uri: Uri,
    addresses: Vec<SocketAddr>,
    request: Request<Body>,
    client_tls: Option<Arc<rustls::ClientConfig>>,
    cancel: CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<Response<Body>, String> {
    let host = uri
        .host()
        .ok_or("missing host")?
        .trim_matches(['[', ']'])
        .to_owned();
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err("invalid HTTP scheme".into());
    }
    let work = async {
        let mut socket = None;
        for (i, address) in addresses.iter().enumerate() {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let timeout = if i + 1 == addresses.len() {
                remaining
            } else {
                (remaining / (addresses.len() - i) as u32).min(Duration::from_secs(2))
            };
            if let Ok(Ok(stream)) =
                tokio::time::timeout(timeout, tokio::net::TcpStream::connect(address)).await
            {
                socket = Some(stream);
                break;
            }
        }
        let socket = socket.ok_or("dial failed")?;
        let io: Box<dyn Io> = if uri.scheme_str() == Some("https") {
            let name = ServerName::try_from(host.clone()).map_err(|_| "invalid TLS hostname")?;
            Box::new(
                tokio_rustls::TlsConnector::from(client_tls.ok_or("TLS policy missing")?)
                    .connect(name, socket)
                    .await
                    .map_err(|_| "TLS handshake failed")?,
            )
        } else {
            Box::new(socket)
        };
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(io))
                .await
                .map_err(|_| "handshake failed")?;
        let mut connection = Box::pin(connection);
        let response = tokio::select! {biased;
            response=sender.send_request(request)=>response.map_err(|_| "request failed")?,
            _=&mut connection=>return Err("connection closed".into()),
        };
        let (parts, body) = response.into_parts();
        let mut body = Box::pin(BodyStream::new(body));
        let mut driving = true;
        let mut finished = false;
        let mut interrupted = Box::pin(cancel.clone().cancelled_owned());
        let mut timeout = Box::pin(tokio::time::sleep_until(deadline));
        let stream = futures_util::stream::poll_fn(move |cx| {
            if finished {
                return Poll::Ready(None);
            }
            if interrupted.as_mut().poll(cx).is_ready() || timeout.as_mut().poll(cx).is_ready() {
                finished = true;
                return Poll::Ready(Some(Err(std::io::Error::other(
                    "metric request cancelled or timed out",
                ))));
            }
            if driving && connection.as_mut().poll(cx).is_ready() {
                driving = false;
            }
            loop {
                match body.as_mut().poll_next(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Ready(Some(Err(_))) => {
                        return Poll::Ready(Some(Err(std::io::Error::other(
                            "metric upstream body failed",
                        ))));
                    }
                    Poll::Ready(Some(Ok(frame))) => {
                        if let Ok(data) = frame.into_data() {
                            return Poll::Ready(Some(Ok::<Bytes, std::io::Error>(data)));
                        }
                    }
                }
            }
        });
        Ok(Response::from_parts(parts, Body::from_stream(stream)))
    };
    tokio::select! {biased;
        _=cancel.cancelled()=>Err("request cancelled".into()),
        _=tokio::time::sleep_until(deadline)=>Err("request timed out".into()),
        result=work=>result,
    }
}
