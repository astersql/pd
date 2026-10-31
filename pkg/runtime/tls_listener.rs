// Copyright 2026 AsterSQL.
//! Host-owned transport selection, sharing the canonical authenticated listener.
use crate::NodeError;
use std::{future::Future, pin::Pin};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(crate) enum Listener {
    Plain(TcpListener),
    Tls(astersql_pd_server::service_auth::Listener),
}
impl Listener {
    pub(crate) fn new(
        tcp: TcpListener,
        url: &str,
        config: &astersql_pd_server_config::Config,
    ) -> Result<Self, NodeError> {
        let tls = &config.security.tls;
        if url.starts_with("https://") {
            Ok(Self::Tls(astersql_pd_server::service_auth::Listener::new(
                tcp, config,
            )?))
        } else if !tls.cert_path.is_empty() || !tls.key_path.is_empty() || !tls.ca_path.is_empty() {
            Err("TLS credentials require an HTTPS client endpoint".into())
        } else {
            Ok(Self::Plain(tcp))
        }
    }
}
pub(crate) fn serve(
    listener: Listener,
    router: axum::Router,
    cancel: CancellationToken,
) -> Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>> {
    match listener {
        Listener::Plain(tcp) => {
            let server = axum::serve(
                tcp,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(cancel.cancelled_owned());
            Box::pin(std::future::IntoFuture::into_future(server))
        }
        Listener::Tls(tls) => {
            let server = axum::serve(tls, router.into_make_service_with_connect_info::<astersql_pd_server::service_auth::PeerAddr>())
                .with_graceful_shutdown(cancel.cancelled_owned());
            Box::pin(std::future::IntoFuture::into_future(server))
        }
    }
}
