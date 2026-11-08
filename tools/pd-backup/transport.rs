// Copyright 2026 AsterSQL.

use std::{fs, path::Path, sync::Arc, time::Duration};

use etcd_client::{Certificate, Client, ConnectOptions, Identity, TlsOptions};
use http_body_util::{BodyExt, Empty, Limited};
use hyper::{Uri, body::Bytes};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client as HttpClient, rt::TokioExecutor};
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use serde_json::Value;

use crate::Result;

pub(crate) struct Credentials {
    ca: Option<Vec<u8>>,
    identity: Option<(Vec<u8>, Vec<u8>)>,
}

impl Credentials {
    pub(crate) fn load(ca: Option<&Path>, identity: Option<(&Path, &Path)>) -> Result<Self> {
        let ca = ca.map(fs::read).transpose()?;
        let identity = identity
            .map(|(cert, key)| -> std::io::Result<_> { Ok((fs::read(cert)?, fs::read(key)?)) })
            .transpose()?;
        Ok(Self { ca, identity })
    }

    pub(crate) async fn connect_etcd(
        &self,
        endpoints: &[Uri],
        timeout: Duration,
    ) -> Result<Client> {
        let mut options = ConnectOptions::new()
            .with_connect_timeout(timeout)
            .with_timeout(timeout)
            .with_require_leader(true);
        if endpoints
            .iter()
            .any(|uri| uri.scheme_str() == Some("https"))
        {
            let mut tls = match &self.ca {
                Some(ca) => TlsOptions::new().ca_certificate(Certificate::from_pem(ca)),
                None => TlsOptions::new().with_native_roots(),
            };
            if let Some((cert, key)) = &self.identity {
                tls = tls.identity(Identity::from_pem(cert, key));
            }
            options = options.with_tls(tls);
        }
        let endpoints: Vec<_> = endpoints.iter().map(Uri::to_string).collect();
        Ok(Client::connect(&endpoints, Some(options)).await?)
    }

    pub(crate) async fn fetch_config(&self, uri: Uri) -> Result<Value> {
        let connector = HttpsConnectorBuilder::new()
            .with_tls_config(self.http_tls(uri.scheme_str() == Some("https"))?)
            .https_or_http()
            .enable_http1()
            .build();
        let client = HttpClient::builder(TokioExecutor::new()).build::<_, Empty<Bytes>>(connector);
        let response = client.get(uri).await?;
        if !response.status().is_success() {
            return Err(format!("PD config returned HTTP {}", response.status()).into());
        }
        let body = Limited::new(response.into_body(), 16 * 1024 * 1024)
            .collect()
            .await?
            .to_bytes();
        let config: Value = serde_json::from_slice(&body)?;
        if !config.is_object() {
            return Err("PD config must be a JSON object".into());
        }
        Ok(config)
    }

    fn http_tls(&self, https: bool) -> Result<ClientConfig> {
        let mut roots = RootCertStore::empty();
        if https {
            if let Some(ca) = &self.ca {
                for cert in CertificateDer::pem_slice_iter(ca) {
                    roots.add(cert?)?;
                }
                if roots.is_empty() {
                    return Err("invalid CA certificate".into());
                }
            } else {
                roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
            }
        }
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_root_certificates(roots);
        match &self.identity {
            Some((cert, key)) if https => {
                let cert =
                    CertificateDer::pem_slice_iter(cert).collect::<std::result::Result<_, _>>()?;
                Ok(config.with_client_auth_cert(cert, PrivateKeyDer::from_pem_slice(key)?)?)
            }
            _ => Ok(config.with_no_client_auth()),
        }
    }
}
