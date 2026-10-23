// Copyright 2026 AsterSQL.

use crate::ConfigScalars;
use astersql_pd_configutil::{ConfigMetaData, adjust_duration, adjust_int64, adjust_string};
use astersql_pd_typeutil::{ByteSize, Duration};
use std::path::{Component, Path, PathBuf};

/// Host facts supplied by the caller, without reading process state.
/// The caller resolves hostname errors and supplies an absolute working directory.
pub struct ConfigDefaultsContext<'a> {
    pub hostname: &'a str,
    pub cwd: &'a Path,
}

impl ConfigScalars {
    /// Applies only Config.Adjust's base value defaults, using literal metadata keys.
    /// This projection does not run full Config validation, nested adjustments,
    /// TSO normalization, or filesystem checks. Hostname and cwd come from the host.
    pub fn adjust_base_defaults(
        &mut self,
        meta: &ConfigMetaData,
        context: ConfigDefaultsContext<'_>,
    ) {
        adjust_string(&mut self.name, &format!("pd-{}", context.hostname));
        adjust_string(&mut self.data_dir, &format!("default.{}", self.name));
        self.data_dir = clean_path(context.cwd, &self.data_dir);
        adjust_string(&mut self.client_urls, "http://127.0.0.1:2379");
        adjust_string(&mut self.advertise_client_urls, &self.client_urls);
        adjust_string(&mut self.peer_urls, "http://127.0.0.1:2380");
        adjust_string(&mut self.advertise_peer_urls, &self.peer_urls);
        if self.initial_cluster.is_empty() {
            self.initial_cluster = self
                .advertise_peer_urls
                .split(',')
                .map(|url| format!("{}={url}", self.name))
                .collect::<Vec<_>>()
                .join(",");
        }
        adjust_string(&mut self.initial_cluster_state, "new");
        adjust_string(&mut self.initial_cluster_token, "pd-cluster");
        adjust_int64(&mut self.leader_lease, 5);
        self.labels.get_or_insert_default();
        adjust_string(&mut self.auto_compaction_mode, "periodic");
        adjust_string(&mut self.auto_compaction_retention, "1h");
        if !meta.is_defined("quota-backend-bytes") {
            self.quota_backend_bytes = ByteSize::from_bytes(8 << 30);
        }
        if !meta.is_defined("max-request-bytes") {
            self.max_request_bytes = 150 << 20;
        }
        adjust_duration(&mut self.tick_interval, Duration::from_nanos(500_000_000));
        adjust_duration(
            &mut self.election_interval,
            Duration::from_nanos(3_000_000_000),
        );
        adjust_duration(
            &mut self.heartbeat_stream_bind_interval,
            Duration::from_nanos(60_000_000_000),
        );
        adjust_duration(
            &mut self.leader_priority_check_interval,
            Duration::from_nanos(60_000_000_000),
        );
        if !meta.is_defined("enable-prevote") {
            self.pre_vote = true;
        }
        if !meta.is_defined("enable-grpc-gateway") {
            self.enable_grpc_gateway = true;
        }
    }
}

// Lexical filepath.Abs/Clean on the host path syntax; never resolves symlinks.
pub(crate) fn clean_path(cwd: &Path, value: &str) -> String {
    let path = cwd.join(value);
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    clean.to_string_lossy().into_owned()
}
