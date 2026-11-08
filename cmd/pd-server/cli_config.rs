// Copyright 2026 AsterSQL.
//! Process-owned argument and file merge; pure defaults remain in server/config.
use crate::classic_process::{Config, ProcessError};
use crate::local_native::LocalNative;
use astersql_pd_server_config::{ConfigDefaultsContext, decode_toml_config};
use std::collections::BTreeMap;

pub(crate) const BOOLEAN_FLAGS: &[&str] = &[
    "--force-new-cluster",
    "--config-check",
    "--etcd-member-exclusive",
];
pub(crate) const STRING_FLAGS: &[&str] = &[
    "--config",
    "--name",
    "--data-dir",
    "--client-urls",
    "--peer-urls",
    "--advertise-client-urls",
    "--advertise-peer-urls",
    "--initial-cluster",
    "--join",
    "--metrics-addr",
    "--log-level",
    "--log-file",
    "--cacert",
    "--cert",
    "--key",
    "--etcd-endpoints",
    "--etcd-member-endpoint",
    "--etcd-member-id",
    LocalNative::DATA_DIR_FLAG,
    LocalNative::CLUSTER_ID_FLAG,
];

pub(crate) fn parse(args: impl Iterator<Item = String>) -> Result<Config, ProcessError> {
    let mut flags = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let (key, value) = if let Some((key, value)) = arg.split_once('=') {
            (key.to_owned(), value.to_owned())
        } else if BOOLEAN_FLAGS.contains(&arg.as_str()) {
            (arg, "true".to_owned())
        } else {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {arg}"))?;
            (arg, value)
        };
        let key = match key.as_str() {
            "-L" => "--log-level".into(),
            other => other.to_owned(),
        };
        if !STRING_FLAGS.contains(&key.as_str()) && !BOOLEAN_FLAGS.contains(&key.as_str()) {
            return Err(format!("unsupported option: {key}").into());
        }
        if BOOLEAN_FLAGS.contains(&key.as_str()) {
            boolean(&value)?;
        }
        flags.insert(key, value);
    }
    // Go's AdjustCommandLineString ignores empty flags; false bools do not
    // override true file values. No PD_* environment value binds these flags.
    let file = flags.get("--config").map(String::as_str).unwrap_or("");
    let input = if file.is_empty() {
        String::new()
    } else {
        std::fs::read_to_string(file)?
    };
    let decoded = decode_toml_config(&input)?;
    let meta = decoded.metadata;
    let mut values = decoded.values;
    if !file.is_empty() {
        values.scalars.adjust_deprecated_from_file(
            file,
            &meta,
            &mut values.log.file.filename,
            &mut values.log.level,
            &mut values.warning_msgs,
        );
    }
    macro_rules! string_flag {
        ($key:literal, $field:expr) => {
            if let Some(value) = flags.get($key).filter(|value| !value.is_empty()) {
                $field = value.clone();
            }
        };
    }
    string_flag!("--name", values.scalars.name);
    string_flag!("--data-dir", values.scalars.data_dir);
    string_flag!("--client-urls", values.scalars.client_urls);
    string_flag!("--peer-urls", values.scalars.peer_urls);
    string_flag!(
        "--advertise-client-urls",
        values.scalars.advertise_client_urls
    );
    string_flag!("--advertise-peer-urls", values.scalars.advertise_peer_urls);
    string_flag!("--initial-cluster", values.scalars.initial_cluster);
    string_flag!("--join", values.scalars.join);
    string_flag!("--metrics-addr", values.metric.push_address);
    string_flag!("--log-level", values.log.level);
    string_flag!("--log-file", values.log.file.filename);
    string_flag!("--cacert", values.security.tls.ca_path);
    string_flag!("--cert", values.security.tls.cert_path);
    string_flag!("--key", values.security.tls.key_path);
    if flags
        .get("--force-new-cluster")
        .map(|s| boolean(s))
        .transpose()?
        .unwrap_or(false)
    {
        values.scalars.force_new_cluster = true;
    }
    let mut unsupported = Vec::new();
    for (key, active) in [
        ("data-dir", !values.scalars.data_dir.is_empty()),
        (
            "initial-cluster",
            !values.scalars.initial_cluster.is_empty(),
        ),
        ("force-new-cluster", values.scalars.force_new_cluster),
    ] {
        if active {
            unsupported.push(key.to_owned());
        }
    }
    let cwd = std::env::current_dir()?;
    let hostname = rustix::system::uname();
    let hostname = hostname.nodename().to_string_lossy();
    values.adjust(
        &meta,
        ConfigDefaultsContext {
            hostname: &hostname,
            cwd: &cwd,
        },
        false,
        &mut Default::default(),
    )?;
    let config_check = flags
        .get("--config-check")
        .map(|s| boolean(s))
        .transpose()?
        .unwrap_or(false);
    let mut config = Config {
        config_check,
        unsupported,
        endpoint: flags.get("--etcd-endpoints").cloned().unwrap_or_default(),
        local_native: None,
        join: values.scalars.join.clone(),
        diagnostic_endpoint: flags.get("--etcd-member-endpoint").cloned(),
        diagnostic_member: flags
            .get("--etcd-member-id")
            .map(|s| s.parse())
            .transpose()?,
        dedicated_member: flags
            .get("--etcd-member-exclusive")
            .map(|s| boolean(s))
            .transpose()?
            .unwrap_or(false),
        client: values.scalars.client_urls.clone(),
        peer: values.scalars.peer_urls.clone(),
        name: values.scalars.name.clone(),
        advertise_client: Some(values.scalars.advertise_client_urls.clone()),
        advertise_peer: Some(values.scalars.advertise_peer_urls.clone()),
        security: values,
    };
    if config.diagnostic_endpoint.is_some() != config.diagnostic_member.is_some()
        || config.diagnostic_member == Some(0)
    {
        return Err(
            "--etcd-member-endpoint and nonzero --etcd-member-id must be supplied together".into(),
        );
    }
    if config.dedicated_member && config.diagnostic_member.is_none() {
        return Err("exclusive ownership requires an explicit etcd member endpoint and ID".into());
    }
    config.local_native = LocalNative::from_flags(&config, &flags)?;
    Ok(config)
}

pub(crate) fn boolean(value: &str) -> Result<bool, ProcessError> {
    match value {
        "true" | "TRUE" | "True" | "1" | "t" | "T" => Ok(true),
        "false" | "FALSE" | "False" | "0" | "f" | "F" => Ok(false),
        _ => Err(format!("invalid boolean: {value}").into()),
    }
}
