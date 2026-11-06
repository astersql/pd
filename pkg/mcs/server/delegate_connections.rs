// Copyright 2026 AsterSQL.
use super::cancellable_io::CancellableIo;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
    time::Duration,
};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tonic::{
    Status,
    transport::{Channel, ClientTlsConfig, Endpoint},
};
use tower::ServiceExt;
#[derive(Clone)]
struct HostExecutor(Handle);
impl<F> hyper::rt::Executor<F> for HostExecutor
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        self.0.spawn(future);
    }
}
/// Host-owned policy refresh for new outbound TLS connections.
pub type ClientTlsFactory =
    Arc<dyn Fn() -> Result<Arc<rustls::ClientConfig>, String> + Send + Sync>;
enum Security {
    Tonic(Option<ClientTlsConfig>),
    Rustls(ClientTlsFactory),
}
trait ConnectionIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> ConnectionIo for T {}
/// Host-owned outbound transports; TLS policy and executor belong to one host.
pub struct DelegateConnections {
    runtime: Handle,
    cancel: CancellationToken,
    tls: Security,
    entries: Mutex<BTreeMap<String, Arc<Connection>>>,
}
impl DelegateConnections {
    pub fn new(runtime: Handle, parent: &CancellationToken, tls: Option<ClientTlsConfig>) -> Self {
        Self {
            runtime,
            cancel: parent.child_token(),
            tls: Security::Tonic(tls),
            entries: Mutex::new(BTreeMap::new()),
        }
    }
    /// Use a caller-owned Rustls policy, including custom peer checks.
    pub fn new_with_rustls(
        runtime: Handle,
        parent: &CancellationToken,
        tls: Arc<rustls::ClientConfig>,
    ) -> Self {
        Self::new_with_rustls_factory(runtime, parent, Arc::new(move || Ok(tls.clone())))
    }
    /// Refresh trust and identity policy only when a cached transport is rebuilt.
    pub fn new_with_rustls_factory(
        runtime: Handle,
        parent: &CancellationToken,
        tls: ClientTlsFactory,
    ) -> Self {
        Self {
            runtime,
            cancel: parent.child_token(),
            tls: Security::Rustls(tls),
            entries: Mutex::new(BTreeMap::new()),
        }
    }
    pub async fn get(&self, target: &str) -> Result<DelegateChannel, Status> {
        let connection = {
            let mut entries = self.entries.lock().unwrap();
            if self.cancel.is_cancelled() {
                return Err(closed());
            }
            entries
                .entry(target.into())
                .or_insert_with(|| {
                    Arc::new(Connection {
                        cancel: self.cancel.child_token(),
                        channel: tokio::sync::OnceCell::new(),
                    })
                })
                .clone()
        };
        connection
            .channel
            .get_or_try_init(|| self.connect(target, connection.cancel.clone()))
            .await?;
        if connection.cancel.is_cancelled() {
            return Err(closed());
        }
        Ok(DelegateChannel {
            connection: Arc::downgrade(&connection),
        })
    }
    async fn connect(&self, target: &str, cancel: CancellationToken) -> Result<Channel, Status> {
        let connection_cancel = cancel.clone();
        let custom = match &self.tls {
            Security::Rustls(factory) => {
                let configured = factory().map_err(Status::unknown)?;
                let mut configured = (*configured).clone();
                configured.alpn_protocols = vec![b"h2".to_vec()];
                Some(Arc::new(configured))
            }
            _ => None,
        };
        let secure = matches!(&self.tls, Security::Tonic(Some(_)) | Security::Rustls(_));
        // Go chooses credentials independently of the URL scheme. The custom
        // connector owns TLS; Tonic receives plain connector IO with a secure origin.
        let uri: axum::http::Uri = target
            .parse()
            .map_err(|error: axum::http::uri::InvalidUri| Status::unknown(error.to_string()))?;
        let mut origin = uri.into_parts();
        origin.scheme = Some(if secure {
            axum::http::uri::Scheme::HTTPS
        } else {
            axum::http::uri::Scheme::HTTP
        });
        let origin = axum::http::Uri::from_parts(origin)
            .map_err(|error| Status::unknown(error.to_string()))?;
        let transport = if custom.is_some() {
            let mut parts = origin.clone().into_parts();
            parts.scheme = Some(axum::http::uri::Scheme::HTTP);
            axum::http::Uri::from_parts(parts)
                .map_err(|error| Status::unknown(error.to_string()))?
        } else {
            origin.clone()
        };
        let mut endpoint = Endpoint::from_shared(transport.to_string())
            .map_err(|error| Status::unknown(error.to_string()))?
            .origin(origin)
            .connect_timeout(Duration::from_secs(3))
            .executor(HostExecutor(self.runtime.clone()));
        if let Security::Tonic(Some(tls)) = &self.tls {
            endpoint = endpoint
                .tls_config(tls.clone())
                .map_err(|error| Status::unknown(error.to_string()))?;
        }
        let connector = tower::service_fn(move |uri: axum::http::Uri| {
            let cancel = connection_cancel.clone();
            let custom = custom.clone();
            async move {
                let host = uri
                    .host()
                    .ok_or_else(|| std::io::Error::other("missing delegate host"))?
                    .trim_matches(['[', ']'])
                    .to_owned();
                let port = uri.port_u16().unwrap_or(if secure { 443 } else { 80 });
                let socket = tokio::select! {biased;
                    _=cancel.cancelled()=>return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted,"delegate connection closed")),
                    socket=tokio::net::TcpStream::connect((host.as_str(),port))=>socket?,
                };
                let io: Box<dyn ConnectionIo> = if let Some(config) = custom {
                    let name = rustls::pki_types::ServerName::try_from(host)
                        .map_err(|_| std::io::Error::other("invalid delegate server name"))?;
                    let connector = tokio_rustls::TlsConnector::from(config);
                    let stream = tokio::select! {biased;
                        _=cancel.cancelled()=>return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted,"delegate connection closed")),
                        stream=connector.connect(name,socket)=>stream?,
                    };
                    if stream.get_ref().1.alpn_protocol() != Some(b"h2") {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "delegate did not negotiate h2",
                        ));
                    }
                    Box::new(stream)
                } else {
                    Box::new(socket)
                };
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(CancellableIo::new(
                    io, cancel,
                )))
            }
        });
        let channel = tokio::select! {biased;
            _=cancel.cancelled()=>return Err(Status::unavailable("delegate connections closed")),
            channel=endpoint.connect_with_connector(connector)=>channel.map_err(|e|Status::unavailable(e.to_string()))?,
        };
        Ok(channel)
    }
    pub fn invalidate(&self, target: &str) {
        if let Some(connection) = self.entries.lock().unwrap().remove(target) {
            connection.cancel.cancel();
        }
    }
    pub fn shutdown(&self) {
        self.cancel.cancel();
        self.entries.lock().unwrap().clear();
    }
}
impl Drop for DelegateConnections {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn closed() -> Status {
    Status::unavailable("delegate connections closed")
}
struct Connection {
    cancel: CancellationToken,
    channel: tokio::sync::OnceCell<Channel>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
/// Cloned clients borrow a host connection without retaining its transport resources.
#[derive(Clone)]
pub struct DelegateChannel {
    connection: Weak<Connection>,
}
impl tower::Service<axum::http::Request<tonic::body::Body>> for DelegateChannel {
    type Response = axum::http::Response<tonic::body::Body>;
    type Error = Status;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Status>> {
        Poll::Ready(
            self.connection
                .upgrade()
                .filter(|c| !c.cancel.is_cancelled())
                .map(|_| ())
                .ok_or_else(closed),
        )
    }
    fn call(&mut self, request: axum::http::Request<tonic::body::Body>) -> Self::Future {
        let connection = self.connection.upgrade();
        Box::pin(async move {
            let connection = connection.ok_or_else(closed)?;
            let channel = connection.channel.get().ok_or_else(closed)?.clone();
            tokio::select! {biased;
                _=connection.cancel.cancelled()=>Err(closed()),
                response=channel.oneshot(request)=>response.map_err(|error|Status::from_error(Box::new(error))),
            }
        })
    }
}
