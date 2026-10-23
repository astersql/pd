// Copyright 2026 AsterSQL.

use crate::ConfigScalars;
use astersql_pd_errs::{TypedError, catalog::ERR_URL_PARSE};
use std::{error::Error, fmt, net::Ipv6Addr};

/// A URL syntax error, missing authority, or invalid join configuration.
#[derive(Debug)]
pub enum ConfigUrlError {
    /// Go net/url rejected an individual URL.
    Parse { raw: String, reason: String },
    /// ParseRequestURI succeeded but scheme or host was empty.
    MissingScheme(String),
    /// A list member failed parsing, wrapped with PD's stable URL error code.
    List(TypedError),
    /// Join validation preserves the original list and underlying error.
    Join { raw: String, source: Box<Self> },
    /// Join and initial-cluster were both supplied.
    ConflictingJoin,
}

impl fmt::Display for ConfigUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { raw, reason } => write!(f, "parse {raw:?}: {reason}"),
            Self::MissingScheme(raw) => write!(f, "{raw} has no scheme"),
            Self::List(error) => error.fmt(f),
            Self::Join { raw, source } => {
                write!(f, "failed to parse join addr:{raw}, err:{source}")
            }
            Self::ConflictingJoin => {
                f.write_str("-initial-cluster and -join can not be provided at the same time")
            }
        }
    }
}

impl Error for ConfigUrlError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::List(error) => Some(error),
            Self::Join { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Validates one request URI and requires both a scheme and a host, like Go
/// ValidateURLWithScheme. Schemes are not restricted to HTTP(S).
pub fn validate_url_with_scheme(raw: &str) -> Result<(), ConfigUrlError> {
    let (scheme, host) = parse(raw, true)?;
    if !scheme || !host {
        return Err(ConfigUrlError::MissingScheme(raw.into()));
    }
    Ok(())
}

/// Validates comma-separated members using Go parseUrls syntax, without trimming
/// or requiring an authority. Empty members and relative paths are accepted.
/// This validates values only; it does not construct or normalize transport URLs.
pub fn validate_url_list(raw: &str) -> Result<(), ConfigUrlError> {
    for item in raw.split(',') {
        if let Err(error) = parse(item, false) {
            return Err(ConfigUrlError::List(
                ERR_URL_PARSE.with_source(std::iter::empty::<String>(), error),
            ));
        }
    }
    Ok(())
}

impl ConfigScalars {
    /// Validates only the join/initial-cluster value contract. Other Config
    /// validation and Adjust defaults remain separate responsibilities.
    pub fn validate_join(&self) -> Result<(), ConfigUrlError> {
        if !self.join.is_empty() && !self.initial_cluster.is_empty() {
            return Err(ConfigUrlError::ConflictingJoin);
        }
        if self.join.is_empty() {
            return Ok(());
        }
        validate_url_list(&self.join).map_err(|error| ConfigUrlError::Join {
            raw: self.join.clone(),
            source: Box::new(error),
        })
    }
}

// Validation branches follow Go 1.26 net/url, not WHATWG URL normalization.
// Query escapes are deliberately unchecked; opaque paths also bypass path checks.
fn parse(raw: &str, request: bool) -> Result<(bool, bool), ConfigUrlError> {
    let (input, fragment) = if request {
        (raw, None)
    } else {
        raw.split_once('#')
            .map_or((raw, None), |(path, fragment)| (path, Some(fragment)))
    };
    let result = parse_inner(input, request).map_err(|reason| ConfigUrlError::Parse {
        raw: input.into(),
        reason,
    })?;
    if let Some(fragment) = fragment {
        unescape(fragment, EscapeMode::Path).map_err(|reason| ConfigUrlError::Parse {
            raw: raw.into(),
            reason,
        })?;
    }
    Ok(result)
}

fn parse_inner(raw: &str, request: bool) -> Result<(bool, bool), String> {
    if raw.bytes().any(|b| b < 32 || b == 127) {
        return Err("net/url: invalid control character in URL".into());
    }
    if raw.is_empty() && request {
        return Err("empty url".into());
    }
    if raw == "*" {
        return Ok((false, false));
    }
    let mut scheme = String::new();
    let mut rest = raw;
    for (i, b) in raw.bytes().enumerate() {
        if b.is_ascii_alphabetic() {
            continue;
        }
        if b.is_ascii_digit() || b"+-.".contains(&b) {
            if i == 0 {
                break;
            }
            continue;
        }
        if b == b':' {
            if i == 0 {
                return Err("missing protocol scheme".into());
            }
            scheme = raw[..i].to_ascii_lowercase();
            rest = &raw[i + 1..];
        }
        break;
    }
    rest = rest.split('?').next().unwrap_or_default();
    if !rest.starts_with('/') {
        if !scheme.is_empty() {
            return Ok((true, false));
        }
        if request {
            return Err("invalid URI for request".into());
        }
        if rest.split('/').next().unwrap_or_default().contains(':') {
            return Err("first path segment in URL cannot contain colon".into());
        }
    }
    let mut has_host = false;
    if (!scheme.is_empty() || (!request && !rest.starts_with("///"))) && rest.starts_with("//") {
        let authority_and_path = &rest[2..];
        let (authority, path) = authority_and_path
            .find('/')
            .map_or((authority_and_path, ""), |i| {
                (&authority_and_path[..i], &authority_and_path[i..])
            });
        let (userinfo, host) = authority
            .rsplit_once('@')
            .map_or((None, authority), |(user, host)| (Some(user), host));
        validate_host(host, &scheme)?;
        if let Some(userinfo) = userinfo {
            if !userinfo
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._:~!$&'()*+,;=%@".contains(&b))
            {
                return Err("net/url: invalid userinfo".into());
            }
            unescape(userinfo, EscapeMode::Path)?;
        }
        has_host = !host.is_empty();
        rest = path;
    }
    unescape(rest, EscapeMode::Path)?;
    Ok((!scheme.is_empty(), has_host))
}

fn validate_port(port: &str) -> Result<(), String> {
    if port.is_empty()
        || port
            .strip_prefix(':')
            .is_some_and(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        Ok(())
    } else {
        Err(format!("invalid port {port:?} after host"))
    }
}

fn validate_host(host: &str, scheme: &str) -> Result<(), String> {
    if let Some(open) = host.rfind('[') {
        if open != 0 {
            return Err("invalid IP-literal".into());
        }
        let close = host.rfind(']').ok_or("missing ']' in host")?;
        validate_port(&host[close + 1..])?;
        unescape(&host[close + 1..], EscapeMode::Host)?;
        let hostname = &host[1..close];
        let decoded = if let Some(zone) = hostname.find("%25") {
            let mut decoded = unescape(&hostname[..zone], EscapeMode::Host)?;
            decoded.extend(unescape(&hostname[zone..], EscapeMode::Zone)?);
            decoded
        } else {
            unescape(hostname, EscapeMode::Host)?
        };
        let literal = String::from_utf8(decoded).map_err(|_| "invalid IP-literal")?;
        let (address, zone) = literal
            .split_once('%')
            .map_or((literal.as_str(), None), |(address, zone)| {
                (address, Some(zone))
            });
        if zone.is_some_and(str::is_empty) || address.parse::<Ipv6Addr>().is_err() {
            return Err("invalid IP-literal".into());
        }
        return Ok(());
    }
    let colon = if matches!(scheme, "http" | "https") {
        host.find(':')
    } else {
        host.rfind(':')
    };
    if let Some(colon) = colon {
        validate_port(&host[colon..])?;
    }
    unescape(host, EscapeMode::Host)?;
    Ok(())
}

#[derive(Clone, Copy)]
enum EscapeMode {
    Path,
    Host,
    Zone,
}

fn host_byte(b: u8) -> bool {
    b >= 128 || b.is_ascii_alphanumeric() || b"!\"$&'()*+,-.:;<=>[]_~".contains(&b)
}

fn unescape(raw: &str, mode: EscapeMode) -> Result<Vec<u8>, String> {
    let input = raw.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let b = input[i];
        if b == b'%' {
            let hex = input
                .get(i + 1..i + 3)
                .filter(|s| s.iter().all(u8::is_ascii_hexdigit));
            let Some(hex) = hex else {
                return Err(format!(
                    "invalid URL escape {:?}",
                    String::from_utf8_lossy(&input[i..input.len().min(i + 3)])
                ));
            };
            let digit = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b.to_ascii_lowercase() - b'a' + 10
                }
            };
            let value = digit(hex[0]) * 16 + digit(hex[1]);
            if (matches!(mode, EscapeMode::Host) && value < 128 && value != b'%')
                || (matches!(mode, EscapeMode::Zone)
                    && value != b'%'
                    && value != b' '
                    && (value >= 128 || !host_byte(value)))
            {
                return Err(format!("invalid URL escape {:?}", &raw[i..i + 3]));
            }
            output.push(value);
            i += 3;
        } else {
            if !matches!(mode, EscapeMode::Path) && !host_byte(b) {
                return Err(format!(
                    "invalid character {:?} in host name",
                    char::from(b).to_string()
                ));
            }
            output.push(b);
            i += 1;
        }
    }
    Ok(output)
}
