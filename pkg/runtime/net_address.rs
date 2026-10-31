// Copyright 2026 AsterSQL.
//! Transport endpoints and owned TCP reservations. Parsing never resolves DNS.
use crate::NodeError;
use std::net::{IpAddr, SocketAddr};
use tokio::net::TcpListener;

/// A validated HTTP(S) endpoint, with normalized DNS/IP and decimal port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    scheme: String,
    host: String,
    port: Option<u16>,
}
impl Endpoint {
    /// Parses an endpoint, rejecting credentials and non-endpoint URL components.
    /// Unlike general configuration URL syntax, transport URLs have no path/query.
    pub fn parse(raw: &str) -> Result<Self, NodeError> {
        let invalid = || -> NodeError { "invalid transport endpoint".into() };
        astersql_pd_server_config::validate_url_with_scheme(raw).map_err(|_| invalid())?;
        let (scheme, authority) = raw.split_once("://").ok_or_else(invalid)?;
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https")
            || authority
                .chars()
                .any(|c| c.is_whitespace() || "@/?#,%\\".contains(c))
        {
            return Err(invalid());
        }
        let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
            let (host, tail) = ipv6.split_once(']').ok_or_else(invalid)?;
            let ip: std::net::Ipv6Addr = host.parse().map_err(|_| invalid())?;
            let port = if tail.is_empty() {
                None
            } else {
                Some(tail.strip_prefix(':').ok_or_else(invalid)?)
            };
            (
                ip.to_ipv4_mapped()
                    .map_or_else(|| ip.to_string(), |ip| ip.to_string()),
                port,
            )
        } else {
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, None), |(h, p)| (h, Some(p)));
            let host = host.to_ascii_lowercase();
            let host = host.strip_suffix('.').unwrap_or(&host);
            if host.is_empty()
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
            {
                return Err(invalid());
            }
            (
                host.parse::<IpAddr>()
                    .map_or_else(|_| host.to_owned(), |ip| ip.to_string()),
                port,
            )
        };
        let port = port
            .map(|p| {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                p.parse::<u16>().map_err(|_| invalid())
            })
            .transpose()?;
        Ok(Self { scheme, host, port })
    }
    fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        self.port
            .map_or_else(|| host.clone(), |p| format!("{host}:{p}"))
    }
    /// Returns the normalized URL without performing DNS lookups.
    pub fn url(&self) -> String {
        format!("{}://{}", self.scheme, self.authority())
    }
    /// Reserves the socket until the returned listener is moved into the host.
    /// The host must wrap HTTPS reservations in a configured TLS listener.
    pub async fn bind(&self) -> Result<TcpListener, NodeError> {
        if self.port.is_none() {
            return Err("listener endpoint requires an explicit port".into());
        }
        Ok(TcpListener::bind((self.host.as_str(), self.port.unwrap())).await?)
    }
    /// Substitutes the assigned port for an explicit zero port. The host stays
    /// explicit: wildcard listeners never silently become loopback advertisements.
    pub fn advertised_url(&self, bound: SocketAddr) -> Result<String, NodeError> {
        self.validate_advertised()?;
        let mut endpoint = self.clone();
        if endpoint.port == Some(0) {
            endpoint.port = Some(bound.port());
        }
        Ok(endpoint.url())
    }
    pub(crate) fn validate_advertised(&self) -> Result<(), NodeError> {
        if self
            .host
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_unspecified())
        {
            return Err("advertised endpoint requires a concrete host".into());
        }
        Ok(())
    }
}
