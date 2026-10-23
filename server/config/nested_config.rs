// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_configutil::{ConfigMetaData, SecurityConfig};
use astersql_pd_logutil::LogConfig;
use astersql_pd_metering_config::MeteringConfig;
use astersql_pd_metricutil::MetricConfig;
use astersql_pd_rm_server::ControllerConfig;
use astersql_pd_schedule_config::{ReplicationConfig, ScheduleConfig, ScheduleDefaults};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Full pure Config values; excludes runtime handles and service startup.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(flatten)]
    pub scalars: ConfigScalars,
    pub log: LogConfig,
    pub metric: MetricConfig,
    #[serde(deserialize_with = "astersql_pd_schedule_config::deserialize_json")]
    pub schedule: ScheduleConfig,
    pub replication: ReplicationConfig,
    #[serde(rename = "pd-server")]
    pub pd_server: PDServerConfig,
    #[serde(rename = "cluster-version")]
    pub cluster_version: ClusterVersion,
    pub security: SecurityConfig,
    #[serde(rename = "label-property")]
    pub label_property: Option<BTreeMap<String, Option<Vec<StoreLabel>>>>,
    pub dashboard: DashboardConfig,
    #[serde(rename = "replication-mode")]
    pub replication_mode: ReplicationModeConfig,
    pub keyspace: KeyspaceConfig,
    #[serde(rename = "micro-service")]
    pub microservice: MicroserviceConfig,
    pub controller: ControllerConfig,
    pub metering: MeteringConfig,
    #[serde(skip)]
    pub warning_msgs: Vec<String>,
}
/// A PD label property value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreLabel {
    pub key: String,
    pub value: String,
}
impl Config {
    /// Applies pure Config.Adjust in Go order, using host facts and host-owned scheduling defaults.
    pub fn adjust(
        &mut self,
        meta: &ConfigMetaData,
        context: ConfigDefaultsContext<'_>,
        reloading: bool,
        defaults: &mut ScheduleDefaults,
    ) -> Result<(), String> {
        use astersql_pd_configutil::{adjust_duration, adjust_string};
        use astersql_pd_typeutil::{ByteSize, Duration};
        if let Some(e) = meta.check_undecoded() {
            self.warning_msgs.push(e);
        }
        adjust_string(&mut self.scalars.name, &format!("pd-{}", context.hostname));
        adjust_string(
            &mut self.scalars.data_dir,
            &format!("default.{}", self.scalars.name),
        );
        self.scalars.data_dir =
            crate::config_defaults::clean_path(context.cwd, &self.scalars.data_dir);
        self.validate(context.cwd)?;
        let c = &mut self.scalars;
        adjust_string(&mut c.client_urls, "http://127.0.0.1:2379");
        adjust_string(&mut c.advertise_client_urls, &c.client_urls);
        adjust_string(&mut c.peer_urls, "http://127.0.0.1:2380");
        adjust_string(&mut c.advertise_peer_urls, &c.peer_urls);
        adjust_duration(
            &mut self.metric.push_interval,
            Duration::from_nanos(15000000000),
        );
        if c.initial_cluster.is_empty() {
            c.initial_cluster = c
                .advertise_peer_urls
                .split(',')
                .map(|url| format!("{}={url}", c.name))
                .collect::<Vec<_>>()
                .join(",");
        }
        adjust_string(&mut c.initial_cluster_state, "new");
        adjust_string(&mut c.initial_cluster_token, "pd-cluster");
        if !c.join.is_empty() {
            validate_url_list(&c.join)
                .map_err(|e| format!("failed to parse join addr:{}, err:{e}", c.join))?;
        }
        c.adjust_tso_defaults();
        astersql_pd_configutil::adjust_int64(&mut c.leader_lease, 5);
        c.labels.get_or_insert_default();
        adjust_string(&mut c.auto_compaction_mode, "periodic");
        adjust_string(&mut c.auto_compaction_retention, "1h");
        if !meta.is_defined("quota-backend-bytes") {
            c.quota_backend_bytes = ByteSize::from_bytes(8 << 30);
        }
        if !meta.is_defined("max-request-bytes") {
            c.max_request_bytes = 150 << 20;
        }
        adjust_duration(&mut c.tick_interval, Duration::from_nanos(500000000));
        adjust_duration(&mut c.election_interval, Duration::from_nanos(3000000000));
        self.metric.adjust(&c.name);
        self.schedule
            .adjust(&meta.child(["schedule"]), reloading, defaults)?;
        self.replication.adjust(&meta.child(["replication"]))?;
        let pd = meta.child(["pd-server"]);
        self.pd_server.adjust_defaults(&pd);
        migrate_configuration_from_file(&pd).map_err(str::to_owned)?;
        self.pd_server.validate().map_err(|e| e.to_string())?;
        self.log
            .adjust(meta.child(["log"]).is_defined("disable-error-verbose"));
        astersql_pd_configutil::adjust_duration(
            &mut self.scalars.heartbeat_stream_bind_interval,
            astersql_pd_typeutil::Duration::from_nanos(60000000000),
        );
        astersql_pd_configutil::adjust_duration(
            &mut self.scalars.leader_priority_check_interval,
            astersql_pd_typeutil::Duration::from_nanos(60000000000),
        );
        if !meta.is_defined("enable-prevote") {
            self.scalars.pre_vote = true;
        }
        if !meta.is_defined("enable-grpc-gateway") {
            self.scalars.enable_grpc_gateway = true;
        }
        self.dashboard.adjust(&meta.child(["dashboard"]));
        self.replication_mode
            .adjust(&meta.child(["replication-mode"]));
        self.keyspace.adjust(&meta.child(["keyspace"]))?;
        self.microservice.adjust(&meta.child(["micro-service"]));
        self.security
            .encryption
            .adjust()
            .map_err(|e| e.to_string())?;
        self.controller.adjust(&meta.child(["controller"]));
        Ok(())
    }
    /// Checks Config.Validate's join and lexical path relation; performs no filesystem IO.
    pub fn validate(&self, cwd: &std::path::Path) -> Result<(), String> {
        if !self.scalars.join.is_empty() && !self.scalars.initial_cluster.is_empty() {
            return Err("-initial-cluster and -join can not be provided at the same time".into());
        }
        let data = crate::config_defaults::clean_path(cwd, &self.scalars.data_dir);
        let log = crate::config_defaults::clean_path(cwd, &self.log.file.filename);
        let log_dir = std::path::Path::new(&log)
            .parent()
            .unwrap_or(std::path::Path::new("/"));
        if let Ok(rel) = log_dir.strip_prefix(&data) {
            // Go checks the raw '..' prefix of filepath.Rel, including names like '..logs'.
            if !rel.to_string_lossy().starts_with("..") {
                return Err("log directory shouldn't be the subdirectory of data directory".into());
            }
        }
        Ok(())
    }
}

/// The pure coreos/semver value, including signed i64 parts accepted by Go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClusterVersion {
    pub major: i64,
    pub minor: i64,
    pub patch: i64,
    pub pre_release: String,
    pub metadata: String,
}
impl std::fmt::Display for ClusterVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre_release.is_empty() {
            write!(f, "-{}", self.pre_release)?;
        }
        if !self.metadata.is_empty() {
            write!(f, "+{}", self.metadata)?;
        }
        Ok(())
    }
}
impl Serialize for ClusterVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for ClusterVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        if text.is_empty() {
            return Ok(Self::default());
        }
        let (number, metadata) = text.split_once('+').unwrap_or((&text, ""));
        let (number, pre) = number.split_once('-').unwrap_or((number, ""));
        for (part, label) in [(pre, "pre-release"), (metadata, "metadata")] {
            if !part.is_empty()
                && !part.split('.').all(|s| {
                    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(serde::de::Error::custom(format!(
                    "failed to validate {label}: {part} is not a valid semver identifier"
                )));
            }
        }
        let parts: Vec<_> = number.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(serde::de::Error::custom(format!(
                "{number} is not in dotted-tri format"
            )));
        }
        let parse = |s: &str| s.parse::<i64>().map_err(serde::de::Error::custom);
        Ok(Self {
            major: parse(parts[0])?,
            minor: parse(parts[1])?,
            patch: parse(parts[2])?,
            pre_release: pre.into(),
            metadata: metadata.into(),
        })
    }
}

/// Decoded Config values and literal TOML presence used by Adjust.
#[derive(Clone, Debug)]
pub struct DecodedConfig {
    pub values: Config,
    pub metadata: ConfigMetaData,
}

/// Decodes fresh JSON values without inventing TOML metadata. Like Go JSON
/// decoding, this does not imply Adjust or persisted-option migration was called.
pub fn decode_json_config(input: &str) -> Result<DecodedConfig, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_str(input)?;
    let Some(object) = raw.as_object() else {
        return Err(serde::de::Error::custom("expected a configuration object"));
    };
    let scalars = decode_json_scalars(input)?.values;
    let mut canonical = canonical_json(object.clone(), &serde_json::to_value(Config::default())?);
    // Struct/map null has no effect on fresh nonpointer structs in Go.
    for key in [
        "log",
        "metric",
        "replication",
        "pd-server",
        "security",
        "dashboard",
        "replication-mode",
        "keyspace",
        "micro-service",
        "controller",
        "metering",
    ] {
        if canonical.get(key).is_some_and(serde_json::Value::is_null) {
            canonical.as_object_mut().unwrap().remove(key);
        }
    }
    for key in serde_json::to_value(ConfigScalars::default())?
        .as_object()
        .unwrap()
        .keys()
    {
        canonical.as_object_mut().unwrap().remove(key);
    }
    let mut values: Config = serde_json::from_value(canonical)?;
    values.scalars = scalars;
    Ok(DecodedConfig {
        values,
        metadata: ConfigMetaData::default(),
    })
}

// Go encoding/json matches tags ignoring ASCII case; preserve map keys verbatim.
fn canonical_json(
    object: serde_json::Map<String, serde_json::Value>,
    schema: &serde_json::Value,
) -> serde_json::Value {
    let Some(shape) = schema.as_object() else {
        return serde_json::Value::Object(object);
    };
    let mut out = serde_json::Map::new();
    for (key, mut value) in object {
        let name = shape
            .keys()
            .find(|n| n.eq_ignore_ascii_case(&key))
            .cloned()
            .or_else(|| json_field(&key).map(str::to_owned))
            .unwrap_or(key);
        if let Some(inner) = value.as_object() {
            if let Some(child) = shape.get(&name) {
                value = canonical_json(inner.clone(), child);
            }
        }
        out.insert(name, value);
    }
    serde_json::Value::Object(out)
}

/// Decodes TOML with native flags/arrays, retaining original key presence.
/// Reuses each already implemented canonical decoder instead of resetting metadata.
pub fn decode_toml_config(input: &str) -> Result<DecodedConfig, String> {
    let original: toml::Table = toml::from_str(input).map_err(|e| e.to_string())?;
    let mut undecoded = Vec::new();
    let table = normalize_toml(original.clone(), "", &mut undecoded);
    let mut paths = Vec::new();
    fn visit(table: &toml::Table, prefix: Vec<String>, out: &mut Vec<Vec<String>>) {
        for (key, value) in table {
            let mut p = prefix.clone();
            p.push(key.clone());
            out.push(p.clone());
            if let Some(t) = value.as_table() {
                visit(t, p, out);
            }
        }
    }
    visit(&original, vec![], &mut paths);
    let scalars = decode_toml_scalars(input)
        .map_err(|e| e.to_string())?
        .values;
    let mut json = serde_json::to_value(&table).map_err(|e| e.to_string())?;
    // Existing specialized decoders below handle their native TOML encodings.
    for key in [
        "schedule",
        "replication",
        "controller",
        "pd-server",
        "dashboard",
        "replication-mode",
        "keyspace",
        "micro-service",
    ] {
        json.as_object_mut().unwrap().remove(key);
    }
    let mut values = decode_json_config(&json.to_string())
        .map_err(|e| e.to_string())?
        .values;
    values.scalars = scalars;
    let section = |key: &str| -> Result<Option<String>, String> {
        table
            .get(key)
            .map(|v| toml::to_string(v).map_err(|e| e.to_string()))
            .transpose()
    };
    if let Some(s) = section("pd-server")? {
        values.pd_server = decode_toml_pd_server(&s).map_err(|e| e.to_string())?.0;
    }
    if let Some(s) = section("dashboard")? {
        values.dashboard = decode_toml_dashboard(&s).map_err(|e| e.to_string())?.0;
    }
    if let Some(s) = section("replication-mode")? {
        values.replication_mode = decode_toml_replication_mode(&s)
            .map_err(|e| e.to_string())?
            .0;
    }
    if let Some(s) = section("keyspace")? {
        values.keyspace = decode_toml_keyspace(&s).map_err(|e| e.to_string())?.0;
    }
    if let Some(s) = section("micro-service")? {
        values.microservice = decode_toml_microservice(&s).map_err(|e| e.to_string())?.0;
    }
    if let Some(s) = section("schedule")? {
        values.schedule = astersql_pd_schedule_config::decode_toml(&s)?.0;
    }
    if let Some(s) = section("replication")? {
        values.replication = astersql_pd_schedule_config::decode_toml_replication(&s)?.0;
    }
    if let Some(s) = section("controller")? {
        values.controller = astersql_pd_rm_server::decode_toml(&s)?.0;
    }
    let metadata = ConfigMetaData::from_defined_paths(paths).with_undecoded_keys(undecoded);
    Ok(DecodedConfig { values, metadata })
}
// Go tags from the pinned source inventory. Keep literal TOML presence separate
// from case-insensitive field matching and JSON's differently named tags.
fn toml_fields(path: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match path {
        "" => Some(&[
            ("client-urls", "client-urls"),
            ("peer-urls", "peer-urls"),
            ("advertise-client-urls", "advertise-client-urls"),
            ("advertise-peer-urls", "advertise-peer-urls"),
            ("name", "name"),
            ("data-dir", "data-dir"),
            ("ForceNewCluster", "force-new-cluster"),
            ("EnableGRPCGateway", "enable-grpc-gateway"),
            ("initial-cluster", "initial-cluster"),
            ("initial-cluster-state", "initial-cluster-state"),
            ("initial-cluster-token", "initial-cluster-token"),
            ("join", "join"),
            ("lease", "lease"),
            ("log", "log"),
            ("log-file", "log-file"),
            ("log-level", "log-level"),
            (
                "max-concurrent-tso-proxy-streamings",
                "max-concurrent-tso-proxy-streamings",
            ),
            (
                "tso-proxy-recv-from-client-timeout",
                "tso-proxy-recv-from-client-timeout",
            ),
            ("tso-save-interval", "tso-save-interval"),
            (
                "tso-update-physical-interval",
                "tso-update-physical-interval",
            ),
            ("enable-local-tso", "enable-local-tso"),
            ("metric", "metric"),
            ("schedule", "schedule"),
            ("replication", "replication"),
            ("pd-server", "pd-server"),
            ("cluster-version", "cluster-version"),
            ("labels", "labels"),
            ("quota-backend-bytes", "quota-backend-bytes"),
            ("auto-compaction-mode", "auto-compaction-mode"),
            ("auto-compaction-retention", "auto-compaction-retention-v2"),
            ("tick-interval", "tick-interval"),
            ("election-interval", "election-interval"),
            ("enable-prevote", "enable-prevote"),
            ("max-request-bytes", "max-request-bytes"),
            ("security", "security"),
            ("label-property", "label-property"),
            ("dashboard", "dashboard"),
            ("replication-mode", "replication-mode"),
            ("keyspace", "keyspace"),
            ("micro-service", "micro-service"),
            ("controller", "controller"),
            ("metering", "metering"),
            ("HeartbeatStreamBindInterval", "HeartbeatStreamBindInterval"),
            ("LeaderPriorityCheckInterval", "LeaderPriorityCheckInterval"),
            ("DisableStrictReconfigCheck", "DisableStrictReconfigCheck"),
        ]),
        "schedule" => Some(&[
            ("max-snapshot-count", "max-snapshot-count"),
            ("max-pending-peer-count", "max-pending-peer-count"),
            ("max-merge-region-size", "max-merge-region-size"),
            ("max-merge-region-keys", "max-merge-region-keys"),
            ("split-merge-interval", "split-merge-interval"),
            ("switch-witness-interval", "switch-witness-interval"),
            ("enable-one-way-merge", "enable-one-way-merge"),
            ("enable-cross-table-merge", "enable-cross-table-merge"),
            ("patrol-region-interval", "patrol-region-interval"),
            ("max-store-down-time", "max-store-down-time"),
            ("max-store-preparing-time", "max-store-preparing-time"),
            ("leader-schedule-limit", "leader-schedule-limit"),
            ("leader-schedule-policy", "leader-schedule-policy"),
            ("region-schedule-limit", "region-schedule-limit"),
            ("witness-schedule-limit", "witness-schedule-limit"),
            ("replica-schedule-limit", "replica-schedule-limit"),
            ("merge-schedule-limit", "merge-schedule-limit"),
            ("hot-region-schedule-limit", "hot-region-schedule-limit"),
            ("affinity-schedule-limit", "affinity-schedule-limit"),
            (
                "split-scatter-schedule-limit",
                "split-scatter-schedule-limit",
            ),
            (
                "hot-region-cache-hits-threshold",
                "hot-region-cache-hits-threshold",
            ),
            ("store-balance-rate", "store-balance-rate"),
            ("default-store-limit", "default-store-limit"),
            ("store-limit", "store-limit"),
            ("tolerant-size-ratio", "tolerant-size-ratio"),
            ("low-space-ratio", "low-space-ratio"),
            ("high-space-ratio", "high-space-ratio"),
            (
                "region-score-formula-version",
                "region-score-formula-version",
            ),
            (
                "scheduler-max-waiting-operator",
                "scheduler-max-waiting-operator",
            ),
            ("disable-raft-learner", "disable-raft-learner"),
            ("disable-remove-down-replica", "disable-remove-down-replica"),
            (
                "disable-replace-offline-replica",
                "disable-replace-offline-replica",
            ),
            ("disable-make-up-replica", "disable-make-up-replica"),
            (
                "disable-remove-extra-replica",
                "disable-remove-extra-replica",
            ),
            (
                "disable-location-replacement",
                "disable-location-replacement",
            ),
            ("enable-remove-down-replica", "enable-remove-down-replica"),
            (
                "enable-replace-offline-replica",
                "enable-replace-offline-replica",
            ),
            ("enable-make-up-replica", "enable-make-up-replica"),
            ("enable-remove-extra-replica", "enable-remove-extra-replica"),
            ("enable-location-replacement", "enable-location-replacement"),
            ("enable-debug-metrics", "enable-debug-metrics"),
            ("enable-joint-consensus", "enable-joint-consensus"),
            ("enable-tikv-split-region", "enable-tikv-split-region"),
            (
                "enable-heartbeat-breakdown-metrics",
                "enable-heartbeat-breakdown-metrics",
            ),
            (
                "enable-heartbeat-concurrent-runner",
                "enable-heartbeat-concurrent-runner",
            ),
            ("schedulers", "schedulers-v2"),
            ("hot-regions-write-interval", "hot-regions-write-interval"),
            ("hot-regions-reserved-days", "hot-regions-reserved-days"),
            ("max-movable-hot-peer-size", "max-movable-hot-peer-size"),
            ("enable-diagnostic", "enable-diagnostic"),
            ("enable-witness", "enable-witness"),
            (
                "slow-store-evicting-affected-store-ratio-threshold",
                "slow-store-evicting-affected-store-ratio-threshold",
            ),
            ("store-limit-version", "store-limit-version"),
            ("halt-scheduling", "halt-scheduling"),
            ("patrol-region-worker-count", "patrol-region-worker-count"),
            (
                "max-affinity-merge-region-size",
                "max-affinity-merge-region-size",
            ),
        ]),
        "schedule.default-store-limit" => Some(&[
            ("add-peer", "add-peer"),
            ("remove-peer", "remove-peer"),
            ("transfer-leader-in", "transfer-leader-in"),
        ]),
        "schedule.schedulers" => Some(&[
            ("type", "type"),
            ("args", "args"),
            ("disable", "disable"),
            ("args-payload", "args-payload"),
        ]),
        "replication" => Some(&[
            ("max-replicas", "max-replicas"),
            ("location-labels", "location-labels"),
            ("strictly-match-label", "strictly-match-label"),
            ("enable-placement-rules", "enable-placement-rules"),
            (
                "enable-placement-rules-cache",
                "enable-placement-rules-cache",
            ),
            ("isolation-level", "isolation-level"),
        ]),
        "controller" => Some(&[
            ("degraded-mode-wait-duration", "degraded-mode-wait-duration"),
            ("ltb-max-wait-duration", "ltb-max-wait-duration"),
            ("ltb-token-rpc-max-delay", "ltb-token-rpc-max-delay"),
            ("request-unit", "request-unit"),
            ("enable-controller-trace-log", "enable-controller-trace-log"),
            ("push-metrics-address", "push-metrics-address"),
            ("push-metrics-interval", "push-metrics-interval"),
            ("ru-version-policy", "ru-version-policy"),
        ]),
        "controller.request-unit" => Some(&[
            ("read-base-cost", "read-base-cost"),
            ("read-per-batch-base-cost", "read-per-batch-base-cost"),
            ("read-cost-per-byte", "read-cost-per-byte"),
            ("write-base-cost", "write-base-cost"),
            ("write-per-batch-base-cost", "write-per-batch-base-cost"),
            ("write-cost-per-byte", "write-cost-per-byte"),
            ("read-cpu-ms-cost", "read-cpu-ms-cost"),
        ]),
        "controller.ru-version-policy" => {
            Some(&[("Default", "default"), ("Overrides", "overrides")])
        }
        "log" => Some(&[
            ("level", "level"),
            ("format", "format"),
            ("disable-timestamp", "disable-timestamp"),
            ("file", "file"),
            ("development", "development"),
            ("disable-caller", "disable-caller"),
            ("disable-stacktrace", "disable-stacktrace"),
            ("disable-error-verbose", "disable-error-verbose"),
            ("sampling", "sampling"),
            ("error-output-path", "error-output-path"),
        ]),
        "log.file" => Some(&[
            ("filename", "filename"),
            ("max-size", "max-size"),
            ("max-days", "max-days"),
            ("max-backups", "max-backups"),
        ]),
        "log.sampling" => Some(&[("Initial", "initial"), ("Thereafter", "thereafter")]),
        "metric" => Some(&[
            ("job", "job"),
            ("address", "address"),
            ("interval", "interval"),
        ]),
        "security" => Some(&[
            ("cacert-path", "cacert-path"),
            ("cert-path", "cert-path"),
            ("key-path", "key-path"),
            ("cert-allowed-cn", "cert-allowed-cn"),
            ("SSLCABytes", "SSLCABytes"),
            ("SSLCertBytes", "SSLCertBytes"),
            ("SSLKEYBytes", "SSLKEYBytes"),
            ("redact-info-log", "redact-info-log"),
            ("encryption", "encryption"),
        ]),
        "security.encryption" => Some(&[
            ("data-encryption-method", "data-encryption-method"),
            ("data-key-rotation-period", "data-key-rotation-period"),
            ("master-key", "master-key"),
        ]),
        "security.encryption.master-key" => Some(&[
            ("type", "type"),
            ("key-id", "key-id"),
            ("region", "region"),
            ("endpoint", "endpoint"),
            ("path", "path"),
        ]),
        "pd-server" => Some(&[
            ("use-region-storage", "use-region-storage"),
            ("max-gap-reset-ts", "max-gap-reset-ts"),
            ("key-type", "key-type"),
            ("runtime-services", "runtime-services"),
            ("metric-storage", "metric-storage"),
            ("dashboard-address", "dashboard-address"),
            ("flow-round-by-digit", "flow-round-by-digit"),
            (
                "min-resolved-ts-persistence-interval",
                "min-resolved-ts-persistence-interval",
            ),
            ("server-memory-limit", "server-memory-limit"),
            (
                "server-memory-limit-gc-trigger",
                "server-memory-limit-gc-trigger",
            ),
            ("enable-gogc-tuner", "enable-gogc-tuner"),
            ("gc-tuner-threshold", "gc-tuner-threshold"),
            ("block-safe-point-v1", "block-safe-point-v1"),
            (
                "gc-keyspace-index-load-concurrency",
                "gc-keyspace-index-load-concurrency",
            ),
            (
                "gc-state-cache-warmup-concurrency",
                "gc-state-cache-warmup-concurrency",
            ),
        ]),
        "dashboard" => Some(&[
            ("tidb-cacert-path", "tidb-cacert-path"),
            ("tidb-cert-path", "tidb-cert-path"),
            ("tidb-key-path", "tidb-key-path"),
            ("public-path-prefix", "public-path-prefix"),
            ("temp-dir", "temp-dir"),
            ("internal-proxy", "internal-proxy"),
            ("enable-telemetry", "enable-telemetry"),
            ("enable-experimental", "enable-experimental"),
            ("disable-custom-prom-addr", "disable-custom-prom-addr"),
        ]),
        "keyspace" => Some(&[
            ("pre-alloc", "pre-alloc"),
            ("wait-region-split", "wait-region-split"),
            ("wait-region-split-timeout", "wait-region-split-timeout"),
            ("check-region-split-interval", "check-region-split-interval"),
            ("meta-service-groups", "meta-service-groups"),
        ]),
        "replication-mode" => Some(&[
            ("replication-mode", "replication-mode"),
            ("dr-auto-sync", "dr-auto-sync"),
        ]),
        "replication-mode.dr-auto-sync" => Some(&[
            ("label-key", "label-key"),
            ("primary", "primary"),
            ("dr", "dr"),
            ("primary-replicas", "primary-replicas"),
            ("dr-replicas", "dr-replicas"),
            ("wait-store-timeout", "wait-store-timeout"),
            ("wait-recover-timeout", "wait-recover-timeout"),
            ("pause-region-split", "pause-region-split"),
        ]),
        "micro-service" => Some(&[
            ("enable-scheduling-fallback", "enable-scheduling-fallback"),
            (
                "enable-tso-dynamic-switching",
                "enable-tso-dynamic-switching",
            ),
            (
                "enable-resource-manager-fallback",
                "enable-resource-manager-fallback",
            ),
        ]),
        "metering" => Some(&[
            ("type", "type"),
            ("region", "region"),
            ("bucket", "bucket"),
            ("prefix", "prefix"),
            ("endpoint", "endpoint"),
            ("aws", "aws"),
            ("oss", "oss"),
            ("cos", "cos"),
            ("azure", "azure"),
            ("localfs", "localfs"),
            ("shared-pool-id", "shared-pool-id"),
        ]),
        "metering.aws" => Some(&[
            ("assume-role-arn", "assume-role-arn"),
            ("s3-force-path-style", "s3-force-path-style"),
            ("access-key", "access-key"),
            ("secret-access-key", "secret-access-key"),
            ("session-token", "session-token"),
        ]),
        "metering.oss" => Some(&[
            ("assume-role-arn", "assume-role-arn"),
            ("access-key", "access-key"),
            ("secret-access-key", "secret-access-key"),
            ("session-token", "session-token"),
        ]),
        "metering.cos" => Some(&[
            ("assume-role-arn", "assume-role-arn"),
            ("access-key", "access-key"),
            ("secret-access-key", "secret-access-key"),
            ("session-token", "session-token"),
        ]),
        "metering.azure" => Some(&[
            ("account-name", "account-name"),
            ("account-key", "account-key"),
            ("sas-token", "sas-token"),
        ]),
        "metering.localfs" => Some(&[
            ("base-path", "base-path"),
            ("create-dirs", "create-dirs"),
            ("permissions", "permissions"),
        ]),
        _ => None,
    }
}
fn normalize_toml(table: toml::Table, path: &str, undecoded: &mut Vec<String>) -> toml::Table {
    let Some(fields) = toml_fields(path) else {
        return table;
    };
    let mut out = toml::Table::new();
    for (key, mut value) in table {
        let found = fields
            .iter()
            .find(|(toml, _)| toml.eq_ignore_ascii_case(&key));
        let name = found.map_or(key.as_str(), |(toml, _)| *toml).to_owned();
        let child = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}.{name}")
        };
        if found.is_none() {
            // TOML Key.String quotes literal dots/special characters.
            let display = if key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                key.clone()
            } else {
                format!("{key:?}")
            };
            undecoded.push(if path.is_empty() {
                display
            } else {
                format!("{path}.{display}")
            });
        }
        if let Some(t) = value.as_table() {
            value = toml::Value::Table(normalize_toml(t.clone(), &child, undecoded));
        }
        if let Some(array) = value.as_array_mut() {
            for item in array {
                if let Some(t) = item.as_table() {
                    *item = toml::Value::Table(normalize_toml(t.clone(), &child, undecoded));
                }
            }
        }
        // TOML uses untagged Go names on fields where JSON has a tag. Scalar
        // decoding handles that mapping; nested decoders use JSON field names.
        let output = if path.is_empty() {
            name
        } else {
            found.map_or(name.clone(), |(_, json)| (*json).into())
        };
        // schedule's owner expects its TOML spelling, then maps to schedulers-v2.
        let output = if path == "schedule" && output == "schedulers-v2" {
            "schedulers".into()
        } else {
            output
        };
        out.insert(output, value);
    }
    out
}
fn json_field(key: &str) -> Option<&'static str> {
    [
        "DisableStrictReconfigCheck",
        "HeartbeatStreamBindInterval",
        "LeaderPriorityCheckInterval",
        "SSLCABytes",
        "SSLCertBytes",
        "SSLKEYBytes",
        "access-key",
        "account-key",
        "account-name",
        "add-peer",
        "address",
        "advertise-client-urls",
        "advertise-peer-urls",
        "affinity-schedule-limit",
        "args",
        "args-payload",
        "assume-role-arn",
        "auto-compaction-mode",
        "auto-compaction-retention-v2",
        "aws",
        "azure",
        "base-path",
        "block-safe-point-v1",
        "bucket",
        "cacert-path",
        "cert-allowed-cn",
        "cert-path",
        "check-region-split-interval",
        "client-urls",
        "cluster-version",
        "controller",
        "cos",
        "create-dirs",
        "dashboard",
        "dashboard-address",
        "data-dir",
        "data-encryption-method",
        "data-key-rotation-period",
        "default",
        "default-store-limit",
        "degraded-mode-wait-duration",
        "development",
        "disable",
        "disable-caller",
        "disable-custom-prom-addr",
        "disable-error-verbose",
        "disable-location-replacement",
        "disable-make-up-replica",
        "disable-raft-learner",
        "disable-remove-down-replica",
        "disable-remove-extra-replica",
        "disable-replace-offline-replica",
        "disable-stacktrace",
        "disable-timestamp",
        "dr",
        "dr-auto-sync",
        "dr-replicas",
        "election-interval",
        "enable-controller-trace-log",
        "enable-cross-table-merge",
        "enable-debug-metrics",
        "enable-diagnostic",
        "enable-experimental",
        "enable-gogc-tuner",
        "enable-grpc-gateway",
        "enable-heartbeat-breakdown-metrics",
        "enable-heartbeat-concurrent-runner",
        "enable-joint-consensus",
        "enable-local-tso",
        "enable-location-replacement",
        "enable-make-up-replica",
        "enable-one-way-merge",
        "enable-placement-rules",
        "enable-placement-rules-cache",
        "enable-prevote",
        "enable-remove-down-replica",
        "enable-remove-extra-replica",
        "enable-replace-offline-replica",
        "enable-resource-manager-fallback",
        "enable-scheduling-fallback",
        "enable-telemetry",
        "enable-tikv-split-region",
        "enable-tso-dynamic-switching",
        "enable-witness",
        "encryption",
        "endpoint",
        "error-output-path",
        "file",
        "filename",
        "flow-round-by-digit",
        "force-new-cluster",
        "format",
        "gc-keyspace-index-load-concurrency",
        "gc-state-cache-warmup-concurrency",
        "gc-tuner-threshold",
        "halt-scheduling",
        "high-space-ratio",
        "hot-region-cache-hits-threshold",
        "hot-region-schedule-limit",
        "hot-regions-reserved-days",
        "hot-regions-write-interval",
        "initial",
        "initial-cluster",
        "initial-cluster-state",
        "initial-cluster-token",
        "internal-proxy",
        "interval",
        "isolation-level",
        "job",
        "join",
        "key-id",
        "key-path",
        "key-type",
        "keyspace",
        "label-key",
        "label-property",
        "labels",
        "leader-schedule-limit",
        "leader-schedule-policy",
        "lease",
        "level",
        "localfs",
        "location-labels",
        "log",
        "log-file",
        "log-level",
        "low-space-ratio",
        "ltb-max-wait-duration",
        "ltb-token-rpc-max-delay",
        "master-key",
        "max-affinity-merge-region-size",
        "max-backups",
        "max-concurrent-tso-proxy-streamings",
        "max-days",
        "max-gap-reset-ts",
        "max-merge-region-keys",
        "max-merge-region-size",
        "max-movable-hot-peer-size",
        "max-pending-peer-count",
        "max-replicas",
        "max-request-bytes",
        "max-size",
        "max-snapshot-count",
        "max-store-down-time",
        "max-store-preparing-time",
        "merge-schedule-limit",
        "meta-service-groups",
        "metering",
        "metric",
        "metric-storage",
        "micro-service",
        "min-resolved-ts-persistence-interval",
        "name",
        "oss",
        "overrides",
        "path",
        "patrol-region-interval",
        "patrol-region-worker-count",
        "pause-region-split",
        "pd-server",
        "peer-urls",
        "permissions",
        "pre-alloc",
        "prefix",
        "primary",
        "primary-replicas",
        "public-path-prefix",
        "push-metrics-address",
        "push-metrics-interval",
        "quota-backend-bytes",
        "read-base-cost",
        "read-cost-per-byte",
        "read-cpu-ms-cost",
        "read-per-batch-base-cost",
        "redact-info-log",
        "region",
        "region-schedule-limit",
        "region-score-formula-version",
        "remove-peer",
        "replica-schedule-limit",
        "replication",
        "replication-mode",
        "request-unit",
        "ru-version-policy",
        "runtime-services",
        "s3-force-path-style",
        "sampling",
        "sas-token",
        "schedule",
        "scheduler-max-waiting-operator",
        "schedulers-v2",
        "secret-access-key",
        "security",
        "server-memory-limit",
        "server-memory-limit-gc-trigger",
        "session-token",
        "shared-pool-id",
        "slow-store-evicting-affected-store-ratio-threshold",
        "split-merge-interval",
        "split-scatter-schedule-limit",
        "store-balance-rate",
        "store-limit",
        "store-limit-version",
        "strictly-match-label",
        "switch-witness-interval",
        "temp-dir",
        "thereafter",
        "tick-interval",
        "tidb-cacert-path",
        "tidb-cert-path",
        "tidb-key-path",
        "tolerant-size-ratio",
        "transfer-leader-in",
        "tso-proxy-recv-from-client-timeout",
        "tso-save-interval",
        "tso-update-physical-interval",
        "type",
        "use-region-storage",
        "wait-recover-timeout",
        "wait-region-split",
        "wait-region-split-timeout",
        "wait-store-timeout",
        "witness-schedule-limit",
        "write-base-cost",
        "write-cost-per-byte",
        "write-per-batch-base-cost",
    ]
    .into_iter()
    .find(|n| n.eq_ignore_ascii_case(key))
}
