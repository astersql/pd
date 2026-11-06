// Copyright 2026 AsterSQL.
//! Host-owned outbound trust and identity, shared by gRPC and HTTP transports.
use astersql_pd_grpcutil::TLSConfig;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
};
use std::sync::Arc;
/// Build caller-provided trust, identity and peer CN policy without a private executor.
/// File credentials reload on full handshakes; byte credentials remain caller-owned values.
pub fn configuration(security: &TLSConfig) -> Result<ClientConfig, String> {
    let mut configured = build_configuration(security)?;
    // Go TLSInfo leaves ClientSessionCache nil. Full reconnects must re-read
    // file identities and authenticate rotated peers instead of resuming old trust.
    configured.resumption = rustls::client::Resumption::disabled();
    Ok(configured)
}
fn build_configuration(security: &TLSConfig) -> Result<ClientConfig, String> {
    let bytes = security.sslca_bytes.as_ref().is_some_and(|b| !b.is_empty())
        || security
            .ssl_cert_bytes
            .as_ref()
            .is_some_and(|b| !b.is_empty())
        || security
            .sslkey_bytes
            .as_ref()
            .is_some_and(|b| !b.is_empty());
    let mut roots = RootCertStore::empty();
    let ca = if bytes {
        Some(security.sslca_bytes.clone().unwrap_or_default())
    } else if security.ca_path.is_empty() {
        None
    } else {
        Some(std::fs::read(&security.ca_path).map_err(|_| "cannot read CA")?)
    };
    if let Some(ca) = ca {
        for cert in CertificateDer::pem_slice_iter(&ca) {
            roots
                .add(cert.map_err(|_| "invalid CA")?)
                .map_err(|_| "invalid CA")?;
        }
        if roots.is_empty() {
            return Err("invalid CA".into());
        }
    } else {
        for cert in rustls_native_certs::load_native_certs().certs {
            roots.add(cert).map_err(|_| "invalid native CA")?;
        }
    }
    let builder =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| "TLS versions")?;
    // Go's raw byte configuration has no TLSInfo callback. File TLSInfo applies
    // every nonempty list, including one CN, after normal chain/hostname checks.
    let names = if bytes || (security.cert_path.is_empty() && security.key_path.is_empty()) {
        vec![]
    } else {
        security.cert_allowed_c_ns.clone().unwrap_or_default()
    };
    let builder = if names.is_empty() {
        builder.with_root_certificates(roots)
    } else {
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .map_err(|_| "invalid CA")?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AllowedServerCN {
                chain: verifier,
                allowed: names,
            }))
    };
    if bytes {
        let certificate = security.ssl_cert_bytes.as_deref().unwrap_or_default();
        let key = security.sslkey_bytes.as_deref().unwrap_or_default();
        let certificates = CertificateDer::pem_slice_iter(certificate)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "invalid client credentials")?;
        let key = PrivateKeyDer::from_pem_slice(key).map_err(|_| "invalid client credentials")?;
        builder
            .with_client_auth_cert(certificates, key)
            .map_err(|_| "invalid client credentials".into())
    } else if security.cert_path.is_empty() && security.key_path.is_empty() {
        Ok(builder.with_no_client_auth())
    } else {
        crate::service_auth::certified_key(&security.cert_path, &security.key_path)
            .ok_or("invalid client credentials")?;
        Ok(builder.with_client_cert_resolver(Arc::new(ReloadClient {
            cert: security.cert_path.clone(),
            key: security.key_path.clone(),
        })))
    }
}
#[derive(Debug)]
struct ReloadClient {
    cert: String,
    key: String,
}
impl rustls::client::ResolvesClientCert for ReloadClient {
    fn resolve(
        &self,
        _: &[&[u8]],
        schemes: &[rustls::SignatureScheme],
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let key = crate::service_auth::certified_key(&self.cert, &self.key)?;
        key.key.choose_scheme(schemes)?;
        Some(key)
    }
    fn has_certs(&self) -> bool {
        true
    }
}
#[derive(Debug)]
struct AllowedServerCN {
    chain: Arc<dyn rustls::client::danger::ServerCertVerifier>,
    allowed: Vec<String>,
}
impl rustls::client::danger::ServerCertVerifier for AllowedServerCN {
    fn verify_server_cert(
        &self,
        end: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let verified = self
            .chain
            .verify_server_cert(end, intermediates, name, ocsp, now)?;
        let cn = crate::service_auth::certificate_common_name(end)?;
        if self.allowed.iter().any(|name| name == &cn) {
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
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.chain.verify_tls12_signature(message, cert, signature)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.chain.verify_tls13_signature(message, cert, signature)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.chain.supported_verify_schemes()
    }
}
