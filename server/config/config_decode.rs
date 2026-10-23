// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::ByteSize;
use serde::{Deserialize, Serialize};

/// The basic scalar projection of Go Config, before Adjust. This is not a full Config.
/// Duration, map and nested configuration fields remain owned by later tasks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigScalars {
    #[serde(rename = "client-urls")]
    pub client_urls: String,
    #[serde(rename = "peer-urls")]
    pub peer_urls: String,
    #[serde(rename = "advertise-client-urls")]
    pub advertise_client_urls: String,
    #[serde(rename = "advertise-peer-urls")]
    pub advertise_peer_urls: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "data-dir")]
    pub data_dir: String,
    #[serde(rename = "force-new-cluster")]
    pub force_new_cluster: bool,
    #[serde(rename = "enable-grpc-gateway")]
    pub enable_grpc_gateway: bool,
    #[serde(rename = "initial-cluster")]
    pub initial_cluster: String,
    #[serde(rename = "initial-cluster-state")]
    pub initial_cluster_state: String,
    #[serde(rename = "initial-cluster-token")]
    pub initial_cluster_token: String,
    #[serde(rename = "join")]
    pub join: String,
    #[serde(rename = "lease")]
    pub leader_lease: i64,
    #[serde(rename = "log-file", skip_serializing_if = "String::is_empty")]
    pub log_file_deprecated: String,
    #[serde(rename = "log-level", skip_serializing_if = "String::is_empty")]
    pub log_level_deprecated: String,
    #[serde(rename = "max-concurrent-tso-proxy-streamings")]
    pub max_concurrent_tso_proxy_streamings: i64,
    #[serde(rename = "enable-local-tso")]
    pub enable_local_tso: bool,
    #[serde(rename = "quota-backend-bytes")]
    pub quota_backend_bytes: ByteSize,
    #[serde(rename = "auto-compaction-mode")]
    pub auto_compaction_mode: String,
    #[serde(rename = "auto-compaction-retention-v2")]
    pub auto_compaction_retention: String,
    #[serde(rename = "enable-prevote")]
    pub pre_vote: bool,
    #[serde(rename = "max-request-bytes")]
    pub max_request_bytes: u64,
}

/// Decoded scalar values and input presence before any default adjustment.
#[derive(Clone, Debug)]
pub struct DecodedScalars {
    pub values: ConfigScalars,
    pub metadata: ConfigMetaData,
}

/// Decodes the scalar projection of a JSON object without applying defaults.
/// Unknown fields are exposed as undecoded metadata and are never passed through.
/// JSON null leaves primitive scalars at zero; ByteSize rejects null like Go.
pub fn decode_json_scalars(input: &str) -> Result<DecodedScalars, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(input)?;
    let Some(object) = value.as_object() else {
        return Err(serde::de::Error::custom("expected a configuration object"));
    };
    decode_object(object, false)
}

/// Decodes the scalar projection of a TOML document without applying defaults.
/// Presence uses the original TOML keys, including Go's untagged field names.
pub fn decode_toml_scalars(input: &str) -> Result<DecodedScalars, toml::de::Error> {
    let table: toml::Table = toml::from_str(input)?;
    let object = serde_json::to_value(table).map_err(serde::de::Error::custom)?;
    decode_object(object.as_object().expect("TOML table is an object"), true)
        .map_err(serde::de::Error::custom)
}

// JSON tag, TOML tag (or untagged Go field name), Go field name.
const FIELDS: &[(&str, &str, &str)] = &[
    ("client-urls", "client-urls", "ClientUrls"),
    ("peer-urls", "peer-urls", "PeerUrls"),
    (
        "advertise-client-urls",
        "advertise-client-urls",
        "AdvertiseClientUrls",
    ),
    (
        "advertise-peer-urls",
        "advertise-peer-urls",
        "AdvertisePeerUrls",
    ),
    ("name", "name", "Name"),
    ("data-dir", "data-dir", "DataDir"),
    ("force-new-cluster", "ForceNewCluster", "ForceNewCluster"),
    (
        "enable-grpc-gateway",
        "EnableGRPCGateway",
        "EnableGRPCGateway",
    ),
    ("initial-cluster", "initial-cluster", "InitialCluster"),
    (
        "initial-cluster-state",
        "initial-cluster-state",
        "InitialClusterState",
    ),
    (
        "initial-cluster-token",
        "initial-cluster-token",
        "InitialClusterToken",
    ),
    ("join", "join", "Join"),
    ("lease", "lease", "LeaderLease"),
    ("log-file", "log-file", "LogFileDeprecated"),
    ("log-level", "log-level", "LogLevelDeprecated"),
    (
        "max-concurrent-tso-proxy-streamings",
        "max-concurrent-tso-proxy-streamings",
        "MaxConcurrentTSOProxyStreamings",
    ),
    ("enable-local-tso", "enable-local-tso", "EnableLocalTSO"),
    (
        "quota-backend-bytes",
        "quota-backend-bytes",
        "QuotaBackendBytes",
    ),
    (
        "auto-compaction-mode",
        "auto-compaction-mode",
        "AutoCompactionMode",
    ),
    (
        "auto-compaction-retention-v2",
        "auto-compaction-retention",
        "AutoCompactionRetention",
    ),
    ("enable-prevote", "enable-prevote", "PreVote"),
    ("max-request-bytes", "max-request-bytes", "MaxRequestBytes"),
];

fn field_name(key: &str, toml: bool) -> Option<&'static str> {
    FIELDS.iter().find_map(|&(json_name, toml_name, _go_name)| {
        let matches = if toml {
            key.eq_ignore_ascii_case(toml_name)
        } else {
            key.eq_ignore_ascii_case(json_name)
        };
        matches.then_some(json_name)
    })
}

fn decode_object(
    object: &serde_json::Map<String, serde_json::Value>,
    toml: bool,
) -> Result<DecodedScalars, serde_json::Error> {
    let mut paths = Vec::new();
    let mut undecoded = Vec::new();
    let mut selected = serde_json::Map::new();
    for (key, value) in object {
        collect_paths(value, vec![key.clone()], &mut paths);
        if let Some(name) = field_name(key, toml) {
            if !value.is_null() || name == "quota-backend-bytes" {
                selected.insert(name.to_owned(), value.clone());
            }
        } else {
            // Includes unimplemented nested Config fields: callers must not
            // mistake this scalar projection for full configuration support.
            undecoded.push(key.clone());
        }
    }
    let values = serde_json::from_value(serde_json::Value::Object(selected))?;
    Ok(DecodedScalars {
        values,
        metadata: ConfigMetaData::from_defined_paths(paths).with_undecoded_keys(undecoded),
    })
}

fn collect_paths(value: &serde_json::Value, path: Vec<String>, paths: &mut Vec<Vec<String>>) {
    paths.push(path.clone());
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            let mut child_path = path.clone();
            child_path.push(key.clone());
            collect_paths(child, child_path, paths);
        }
    }
}
