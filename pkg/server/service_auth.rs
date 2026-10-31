// Copyright 2026 AsterSQL.
//! Client-facing PD authentication: TLS chain validation and Go's CN allowlist.
//! Authentication happens before HTTP routing, RPC dispatch, or forwarding.
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

/// Host-owned authenticated listener. Dropping it aborts pending handshakes.
/// The host supplies the existing Config; no credentials are taken from headers.
pub struct Listener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
    pending: JoinSet<(Result<TlsStream<TcpStream>, std::io::Error>, SocketAddr)>,
}
impl Listener {
    /// Construct the TLS client endpoint from PD security configuration.
    /// A CA path enables mandatory client certificates, as in GenEmbedEtcdConfig.
    /// A zero/one-element CN list intentionally does not restrict client CNs.
    pub fn new(
        tcp: TcpListener,
        config: &astersql_pd_server_config::Config,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let security = &config.security.tls;
        let cert = std::fs::read(&security.cert_path)?;
        let key = std::fs::read(&security.key_path)?;
        let certs = CertificateDer::pem_slice_iter(&cert).collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_slice(&key)?;
        let builder =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?;
        let builder = if security.ca_path.is_empty() {
            builder.with_no_client_auth()
        } else {
            let ca = std::fs::read(&security.ca_path)?;
            let mut roots = RootCertStore::empty();
            for cert in CertificateDer::pem_slice_iter(&ca) {
                roots.add(cert?)?;
            }
            let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
                Arc::new(roots),
                Arc::new(rustls::crypto::ring::default_provider()),
            )
            .build()?;
            let allowed = security.cert_allowed_c_ns.clone().unwrap_or_default();
            let verifier: Arc<dyn rustls::server::danger::ClientCertVerifier> = if allowed.len() > 1
            {
                Arc::new(AllowedCN {
                    chain: verifier,
                    allowed,
                })
            } else {
                verifier
            };
            builder.with_client_cert_verifier(verifier)
        };
        // Validate at startup, then re-read the pair on every full handshake.
        let _ = rustls::sign::CertifiedKey::from_der(
            certs,
            key,
            &rustls::crypto::ring::default_provider(),
        )?;
        let mut tls = builder.with_cert_resolver(Arc::new(ReloadCertificate {
            cert: security.cert_path.clone(),
            key: security.key_path.clone(),
        }));
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(Self {
            tcp,
            acceptor: TlsAcceptor::from(Arc::new(tls)),
            pending: JoinSet::new(),
        })
    }
}
impl axum::serve::Listener for Listener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            tokio::select! {
                result = self.pending.join_next(), if !self.pending.is_empty() => {
                    if let Some(Ok((Ok(stream), addr))) = result { return (stream, addr); }
                }
                result = self.tcp.accept(), if self.pending.len() < 128 => {
                    match result {
                        Ok((tcp, addr)) => {
                            let acceptor = self.acceptor.clone();
                            self.pending.spawn(async move {
                                let result = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await.unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()));
                                (result, addr)
                            });
                        }
                        Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                    }
                }
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}
/// Authenticated connection address, supplied by the TLS listener, never headers.
#[derive(Clone, Copy)]
pub struct PeerAddr(pub SocketAddr);
impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, Listener>>
    for PeerAddr
{
    fn connect_info(stream: axum::serve::IncomingStream<'_, Listener>) -> Self {
        Self(*stream.remote_addr())
    }
}

#[derive(Debug)]
struct AllowedCN {
    chain: Arc<dyn rustls::server::danger::ClientCertVerifier>,
    allowed: Vec<String>,
}
impl rustls::server::danger::ClientCertVerifier for AllowedCN {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        self.chain.root_hint_subjects()
    }
    fn verify_client_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        let verified = self.chain.verify_client_cert(end, intermediates, now)?;
        let (_, cert) = x509_parser::parse_x509_certificate(end.as_ref()).map_err(|_| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;
        // Go pkix.Name.CommonName contains the last CN in the subject.
        let cn = cert
            .subject()
            .iter_common_name()
            .last()
            .map(|cn| {
                let value = cn.attr_value();
                match value.tag() {
                    x509_parser::asn1_rs::Tag::TeletexString => {
                        Ok(value.data.iter().map(|b| char::from(*b)).collect())
                    }
                    x509_parser::asn1_rs::Tag::BmpString => {
                        if value.data.len() % 2 != 0 {
                            return Err(());
                        }
                        let mut points = value
                            .data
                            .chunks_exact(2)
                            .map(|s| u16::from_be_bytes([s[0], s[1]]))
                            .collect::<Vec<_>>();
                        if points.last() == Some(&0) {
                            points.pop();
                        }
                        if points.iter().any(
                            |p| matches!(*p,0xd800..=0xdfff | 0xfdd0..=0xfdef | 0xfffe..=0xffff),
                        ) {
                            return Err(());
                        }
                        String::from_utf16(&points).map_err(|_| ())
                    }
                    _ => cn.as_str().map(str::to_owned).map_err(|_| ()),
                }
            })
            .transpose()
            .map_err(|_| rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?
            .unwrap_or_default();
        if self.allowed.iter().any(|allowed| allowed == &cn) {
            Ok(verified)
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.chain.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.chain.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.chain.supported_verify_schemes()
    }
}

/// Matches etcd TLSInfo.GetCertificate: errors reject this handshake only.
/// Trust roots and client identity policy remain fixed for this listener.
#[derive(Debug)]
struct ReloadCertificate {
    cert: String,
    key: String,
}
impl rustls::server::ResolvesServerCert for ReloadCertificate {
    fn resolve(
        &self,
        _: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let cert = std::fs::read(&self.cert).ok()?;
        let key = std::fs::read(&self.key).ok()?;
        let certs = CertificateDer::pem_slice_iter(&cert)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let key = PrivateKeyDer::from_pem_slice(&key).ok()?;
        rustls::sign::CertifiedKey::from_der(certs, key, &rustls::crypto::ring::default_provider())
            .ok()
            .map(Arc::new)
    }
}
