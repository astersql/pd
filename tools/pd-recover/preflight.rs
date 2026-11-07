// Copyright 2026 AsterSQL.
//! Parse the PD recover command and validate local input before any RPC.

use std::{fs, path::PathBuf, time::Duration};

use etcd_client::{Certificate, Client, ConnectOptions, Identity, TlsOptions};

use crate::{Result, rejected};

/// Supported command-line options.
pub const USAGE: &str = "Usage: pd-recover [-V] [-endpoints URLS] [-cluster-id ID -alloc-id ID [-alloc-timestamp NS] | -from-old-member] [-cacert PATH] [-cert PATH -key PATH]\n       pd-recover --native-restore ARCHIVE NEW_DIRECTORY NEW_CLUSTER_ID NODE_ID";

/// Validated action for the recovery binary.
pub enum Action {
    Help,
    Version,
    Recover(Options),
    NativeRestore {
        archive: PathBuf,
        directory: PathBuf,
        cluster: u64,
        node: u64,
    },
}

/// Connection and recovery inputs.
pub struct Options {
    endpoints: Vec<String>,
    tls: Option<TlsOptions>,
    /// Recover metadata in place when true.
    pub from_old_member: bool,
    /// Required ID when recovering an empty cluster.
    pub cluster_id: u64,
    /// Required ID allocator boundary when recovering an empty cluster.
    pub alloc_id: u64,
    /// Optional persisted timestamp from a metadata backup, in Unix nanoseconds.
    pub alloc_timestamp: Option<u64>,
}

impl Options {
    /// Connect using the validated endpoints and credentials.
    pub async fn connect(&self) -> Result<Client> {
        let mut options = ConnectOptions::new()
            .with_connect_timeout(Duration::from_secs(3))
            .with_timeout(Duration::from_secs(10))
            .with_require_leader(true);
        if let Some(tls) = &self.tls {
            options = options.with_tls(tls.clone());
        }
        Ok(tokio::time::timeout(
            Duration::from_secs(3),
            Client::connect(&self.endpoints, Some(options)),
        )
        .await??)
    }
}

/// Parse Go-style flags without connecting to the metadata service.
pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Action> {
    let mut args = arguments.into_iter().peekable();
    if args.peek().is_none() {
        return Ok(Action::Help);
    }
    if args.peek().is_some_and(|arg| arg == "--native-restore") {
        args.next();
        let mut required = || {
            args.next()
                .ok_or_else(|| rejected("missing native restore argument"))
        };
        let archive = required()?.into();
        let directory = required()?.into();
        let cluster = required()?.parse()?;
        let node = required()?.parse()?;
        if cluster == 0 || node == 0 || args.next().is_some() {
            return Err(rejected("invalid native restore arguments"));
        }
        return Ok(Action::NativeRestore {
            archive,
            directory,
            cluster,
            node,
        });
    }
    let mut endpoints = "http://127.0.0.1:2379".to_owned();
    let (mut ca, mut cert, mut key) = (None, None, None);
    let (mut cluster_id, mut alloc_id) = (0, 0);
    let mut alloc_timestamp = None;
    let (mut from_old_member, mut version) = (false, false);
    while let Some(arg) = args.next() {
        let flag = arg
            .strip_prefix("--")
            .or_else(|| arg.strip_prefix('-'))
            .ok_or_else(|| rejected("expected an option"))?;
        let (name, inline) = flag
            .split_once('=')
            .map_or((flag, None), |(name, value)| (name, Some(value)));
        let mut value = || {
            inline
                .map(str::to_owned)
                .or_else(|| args.next())
                .ok_or_else(|| rejected("missing option value"))
        };
        match name {
            "h" | "help" => return Ok(Action::Help),
            "V" => version = inline.map(str::parse).transpose()?.unwrap_or(true),
            "from-old-member" => {
                from_old_member = inline.map(str::parse).transpose()?.unwrap_or(true)
            }
            "endpoints" => endpoints = value()?,
            "cluster-id" => cluster_id = value()?.parse()?,
            "alloc-id" => alloc_id = value()?.parse()?,
            "alloc-timestamp" => alloc_timestamp = Some(value()?.parse()?),
            "cacert" => ca = Some(value()?),
            "cert" => cert = Some(value()?),
            "key" => key = Some(value()?),
            _ => return Err(rejected("unknown recovery option")),
        }
    }
    if version {
        return Ok(Action::Version);
    }
    if !from_old_member && cluster_id == 0 {
        return Err(rejected("please specify safe cluster-id"));
    }
    if !from_old_member && alloc_id == 0 {
        return Err(rejected("please specify safe alloc-id"));
    }
    if alloc_timestamp.is_some_and(|n| n == 0 || n > i64::MAX as u64 || from_old_member) {
        return Err(rejected(
            "alloc-timestamp requires a valid new-cluster timestamp",
        ));
    }
    let endpoints: Vec<String> = endpoints.split(',').map(str::to_owned).collect();
    if endpoints.iter().any(String::is_empty) {
        return Err(rejected("empty etcd endpoint"));
    }
    if cert.is_some() != key.is_some() {
        return Err(rejected("cert and key must be supplied together"));
    }
    let tls = if ca.is_some() || cert.is_some() {
        let mut tls = TlsOptions::new();
        if let Some(path) = ca {
            tls = tls.ca_certificate(Certificate::from_pem(fs::read(path)?));
        }
        if let (Some(cert), Some(key)) = (cert, key) {
            tls = tls.identity(Identity::from_pem(fs::read(cert)?, fs::read(key)?));
        }
        Some(tls)
    } else {
        None
    };
    Ok(Action::Recover(Options {
        endpoints,
        tls,
        from_old_member,
        cluster_id,
        alloc_id,
        alloc_timestamp,
    }))
}
