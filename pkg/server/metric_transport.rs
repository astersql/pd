// Copyright 2026 AsterSQL.
//! Request-owned HTTP driver with pinned DNS and bounded, cancellable streaming.
use axum::{
    body::{Body, Bytes},
    http::{Request, Response, Uri},
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

pub(crate) struct Target {
    pub uri: Uri,
    host: String,
    port: u16,
    pub authorization: Option<axum::http::HeaderValue>,
}
impl Target {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let (scheme, rest) = raw.split_once("://").ok_or("invalid URL")?;
        if raw.contains('#') || !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return Err("invalid URL".into());
        }
        let uri: Uri = format!("{}://{rest}", scheme.to_ascii_lowercase())
            .parse()
            .map_err(|_| "invalid URL")?;
        let authority = uri.authority().ok_or("missing authority")?;
        if authority.as_str().contains('@') || authority.as_str().ends_with(':') {
            return Err("invalid authority".into());
        }
        let host = uri
            .host()
            .ok_or("missing host")?
            .trim_matches(['[', ']'])
            .to_string();
        if host.is_empty() || host.contains('%') {
            return Err("invalid host".into());
        }
        let port = if authority.as_str().ends_with(']')
            || !authority
                .as_str()
                .trim_start_matches('[')
                .rsplit(']')
                .next()
                .unwrap_or("")
                .contains(':')
                && !authority.as_str().contains(':')
        {
            if uri.scheme_str() == Some("https") {
                443
            } else {
                80
            }
        } else {
            uri.port_u16().ok_or("invalid port")?
        };
        if port == 0 {
            return Err("invalid port".into());
        }
        Ok(Self {
            uri,
            host,
            port,
            authorization: None,
        })
    }
    pub fn parse_push(raw: &str) -> Result<Self, String> {
        let (scheme, rest) = raw.split_once("://").ok_or("invalid URL")?;
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..end];
        let Some((user, host)) = authority.rsplit_once('@') else {
            return Self::parse(raw);
        };
        fn decode(value: &str) -> Result<Vec<u8>, String> {
            let mut bytes = Vec::new();
            let mut input = value.as_bytes().iter().copied();
            while let Some(b) = input.next() {
                if b == b'%' {
                    let a = input
                        .next()
                        .and_then(|b| (b as char).to_digit(16))
                        .ok_or("invalid credentials")?;
                    let b = input
                        .next()
                        .and_then(|b| (b as char).to_digit(16))
                        .ok_or("invalid credentials")?;
                    bytes.push((a * 16 + b) as u8);
                } else {
                    bytes.push(b);
                }
            }
            Ok(bytes)
        }
        let (name, password) = user.split_once(':').unwrap_or((user, ""));
        let mut credentials = decode(name)?;
        credentials.push(b':');
        credentials.extend(decode(password)?);
        use base64::Engine;
        let mut target = Self::parse(&format!("{scheme}://{host}{}", &rest[end..]))?;
        target.authorization = Some(
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(credentials)
            )
            .parse()
            .map_err(|_| "invalid credentials")?,
        );
        Ok(target)
    }
    pub async fn addresses(&self, restricted: bool) -> Result<Vec<SocketAddr>, String> {
        let addresses: Vec<_> = tokio::net::lookup_host((self.host.as_str(), self.port))
            .await
            .map_err(|_| "DNS failed")?
            .map(|a| SocketAddr::new(a.ip().to_canonical(), a.port()))
            .collect();
        if addresses.is_empty() || restricted && addresses.iter().any(|a| !permitted(a.ip())) {
            return Err("destination denied".into());
        }
        Ok(addresses)
    }
}
pub(crate) fn permitted(ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return false;
    }
    match ip {
        IpAddr::V4(v) => {
            let a = v.octets();
            !v.is_link_local()
                && a[0] != 0
                && a[0] < 224
                && !matches!(a, [100, 100, 100, 200] | [192, 0, 0, 192])
        }
        IpAddr::V6(v) => {
            !v.is_unicast_link_local()
                && !["fd00:ec2::254", "fd20:ce::254"]
                    .iter()
                    .any(|s| s.parse::<IpAddr>().ok() == Some(ip))
        }
    }
}
pub(crate) fn tls(
    config: &astersql_pd_server_config::Config,
) -> Result<rustls::ClientConfig, String> {
    crate::client_auth::configuration(&config.security.tls)
}
pub(crate) async fn send(
    target: Target,
    request: Request<Body>,
    config: &astersql_pd_server_config::Config,
    restricted: bool,
    client_tls: Option<Arc<rustls::ClientConfig>>,
    cancel: CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<Response<Body>, String> {
    let work = async {
        let addresses = target.addresses(restricted).await?;
        let client_tls = if target.uri.scheme_str() == Some("https") {
            Some(match client_tls {
                Some(tls) => tls,
                None => Arc::new(tls(config)?),
            })
        } else {
            None
        };
        astersql_pd_mcs_server::http_transport::send(
            target.uri,
            addresses,
            request,
            client_tls,
            cancel.clone(),
            deadline,
        )
        .await
    };
    tokio::select! {biased;
        _=cancel.cancelled()=>Err("request cancelled".into()),
        _=tokio::time::sleep_until(deadline)=>Err("request timed out".into()),
        result=work=>result,
    }
}
pub(crate) fn strip_hop(headers: &mut axum::http::HeaderMap) {
    let names: Vec<_> = headers
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|s| s.trim().to_owned())
        .collect();
    for name in names {
        headers.remove(name);
    }
    for name in [
        "connection",
        "proxy-connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
    ] {
        headers.remove(name);
    }
}

pub(crate) fn gunzip(body: Body) -> Body {
    use futures_util::StreamExt;
    use std::io::Write;
    let stream = futures_util::stream::try_unfold(
        (
            Box::pin(body.into_data_stream()),
            flate2::write::GzDecoder::new(Vec::new()),
            false,
        ),
        |(mut input, mut decoder, mut ended)| async move {
            if ended {
                return Ok(None);
            }
            loop {
                match input.next().await {
                    Some(Ok(bytes)) => decoder.write_all(&bytes)?,
                    Some(Err(_)) => {
                        return Err(std::io::Error::other("metric compressed body failed"));
                    }
                    None => {
                        decoder.try_finish()?;
                        ended = true;
                    }
                }
                let bytes = std::mem::take(decoder.get_mut());
                if !bytes.is_empty() {
                    return Ok(Some((Bytes::from(bytes), (input, decoder, ended))));
                }
                if ended {
                    return Ok(None);
                }
            }
        },
    );
    Body::from_stream(stream)
}
