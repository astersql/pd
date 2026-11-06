// Copyright 2026 AsterSQL.
//! Process-owned common MCS file/flag merge and listener transport selection.
use astersql_pd_configutil::SecurityConfig;
use astersql_pd_logutil::LogConfig;
use serde::Deserialize;
use std::{collections::BTreeMap, net::SocketAddr};
use tokio::net::TcpListener;
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub(crate) struct Config {
    pub backend_endpoints: String,
    pub listen_addr: String,
    pub advertise_listen_addr: String,
    pub name: String,
    pub security: SecurityConfig,
    pub log: LogConfig,
    #[serde(skip)]
    pub controller: astersql_pd_rm_server::ControllerConfig,
}
impl Config {
    pub(crate) fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut flags = BTreeMap::new();
        let mut it = args.into_iter();
        while let Some(arg) = it.next() {
            let (key, value) = if let Some((key, value)) = arg.split_once('=') {
                (key.to_owned(), value.to_owned())
            } else {
                let value = it
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                (arg, value)
            };
            if !matches!(
                key.as_str(),
                "--config"
                    | "--backend-endpoints"
                    | "--listen-addr"
                    | "--advertise-listen-addr"
                    | "--name"
                    | "--cacert"
                    | "--cert"
                    | "--key"
                    | "--log-level"
                    | "--log-file"
            ) {
                return Err(format!("unsupported microservice option: {key}"));
            }
            flags.insert(key, value);
        }
        let source = if let Some(path) = flags.get("--config").filter(|path| !path.is_empty()) {
            std::fs::read_to_string(path).map_err(|e| e.to_string())?
        } else {
            String::new()
        };
        let mut config: Self = toml::from_str(&source).map_err(|e| e.to_string())?;
        // Reuse the canonical decoders for TOML forms and presence-sensitive
        // controller defaults, including explicitly configured zero costs.
        let metadata = astersql_pd_server_config::decode_toml_scalars(&source)
            .map_err(|e| e.to_string())?
            .metadata;
        let table: toml::Table = toml::from_str(&source).map_err(|e| e.to_string())?;
        if let Some(controller) = table.get("controller") {
            let source = toml::to_string(controller).map_err(|e| e.to_string())?;
            let (mut controller, metadata) = astersql_pd_rm_server::decode_toml(&source)?;
            controller.adjust(&metadata);
            config.controller = controller;
        } else {
            config.controller.adjust(&Default::default());
        }
        macro_rules! flag {
            ($name:literal,$field:expr) => {
                if let Some(value) = flags.get($name).filter(|value| !value.is_empty()) {
                    $field = value.clone();
                }
            };
        }
        flag!("--backend-endpoints", config.backend_endpoints);
        flag!("--listen-addr", config.listen_addr);
        flag!("--advertise-listen-addr", config.advertise_listen_addr);
        flag!("--name", config.name);
        flag!("--cacert", config.security.tls.ca_path);
        flag!("--cert", config.security.tls.cert_path);
        flag!("--key", config.security.tls.key_path);
        flag!("--log-level", config.log.level);
        flag!("--log-file", config.log.file.filename);
        if config.backend_endpoints.is_empty() {
            config.backend_endpoints = "http://127.0.0.1:2379".into();
        }
        if config.listen_addr.is_empty() {
            config.listen_addr = "http://127.0.0.1:3379".into();
        }
        if config.advertise_listen_addr.is_empty() {
            config.advertise_listen_addr = config.listen_addr.clone();
        }
        if config.name.is_empty() {
            config.name = config.advertise_listen_addr.clone();
        }
        config
            .log
            .adjust(metadata.child(["log"]).is_defined("disable-error-verbose"));
        config
            .security
            .encryption
            .adjust()
            .map_err(|e| e.to_string())?;
        Ok(config)
    }
    pub(crate) fn listener(&self, tcp: TcpListener) -> Result<Listener, String> {
        if self.security.tls.cert_path.is_empty() && self.security.tls.key_path.is_empty() {
            Ok(Listener::Plain(tcp))
        } else {
            astersql_pd_server::service_auth::Listener::for_microservice(tcp, &self.security.tls)
                .map(Listener::Secure)
                .map_err(|e| e.to_string())
        }
    }
    pub(crate) fn has_client_credentials(&self) -> bool {
        let tls = &self.security.tls;
        !tls.cert_path.is_empty()
            || !tls.key_path.is_empty()
            || [&tls.sslca_bytes, &tls.ssl_cert_bytes, &tls.sslkey_bytes]
                .into_iter()
                .any(|value| value.as_ref().is_some_and(|bytes| !bytes.is_empty()))
    }
    pub(crate) fn validate_client_credentials(&self) -> Result<(), String> {
        if self.has_client_credentials() {
            astersql_pd_server::client_auth::configuration(&self.security.tls)?;
        }
        Ok(())
    }
    pub(crate) fn client_tls_policy(&self) -> astersql_pd_mcs_server::ClientTlsFactory {
        let security = if self.has_client_credentials() {
            self.security.tls.clone()
        } else {
            Default::default()
        };
        std::sync::Arc::new(move || {
            astersql_pd_server::client_auth::configuration(&security).map(std::sync::Arc::new)
        })
    }
    pub(crate) fn delegates(
        &self,
        runtime: tokio::runtime::Handle,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<std::sync::Arc<astersql_pd_mcs_server::DelegateConnections>, String> {
        let pool = if self.has_client_credentials() {
            self.validate_client_credentials()?;
            astersql_pd_mcs_server::DelegateConnections::new_with_rustls_factory(
                runtime,
                cancel,
                self.client_tls_policy(),
            )
        } else {
            astersql_pd_mcs_server::DelegateConnections::new(runtime, cancel, None)
        };
        Ok(std::sync::Arc::new(pool))
    }
}
pub(crate) trait StreamIo:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send
{
}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> StreamIo for T {}
pub(crate) enum Listener {
    Plain(TcpListener),
    Secure(astersql_pd_server::service_auth::Listener),
}
impl axum::serve::Listener for Listener {
    type Io = Box<dyn StreamIo>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self {
            Self::Plain(listener) => {
                let (io, addr) = axum::serve::Listener::accept(listener).await;
                (Box::new(io), addr)
            }
            Self::Secure(listener) => {
                let (io, addr) = axum::serve::Listener::accept(listener).await;
                (Box::new(io), addr)
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        match self {
            Self::Plain(listener) => listener.local_addr(),
            Self::Secure(listener) => axum::serve::Listener::local_addr(listener),
        }
    }
}
