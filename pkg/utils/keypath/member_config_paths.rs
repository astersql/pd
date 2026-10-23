// Copyright 2026 AsterSQL.
//! Configuration, member and election keys from the Go keypath package.
use crate::ClusterPaths;

/// Microservice election parameters. Group IDs only affect the `tso` service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MsParam {
    /// Service name, preserved without path normalization.
    pub service_name: String,
    /// TSO keyspace group ID.
    pub group_id: u32,
}

/// Returns Go path.Dir semantics, independent of the host OS path separator.
pub fn prefix(path: &str) -> String {
    let dir = path.rfind('/').map_or("", |i| &path[..=i]);
    let rooted = dir.starts_with('/');
    let mut parts = Vec::new();
    for part in dir.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else if !rooted {
                    parts.push("..");
                }
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    }
}

impl ClusterPaths {
    /// Returns the PD configuration key.
    pub fn config_path(&self) -> String {
        format!("/pd/{}/config", self.cluster_id())
    }
    /// Returns the scheduler configuration prefix, including its trailing slash.
    pub fn scheduler_config_path_prefix(&self) -> String {
        self.scheduler_config_path("")
    }
    /// Returns the scheduler configuration key, preserving the name verbatim.
    pub fn scheduler_config_path(&self, name: &str) -> String {
        format!("/pd/{}/scheduler_config/{name}", self.cluster_id())
    }
    /// Returns the service middleware configuration key.
    pub fn service_middleware_path(&self) -> String {
        format!("/pd/{}/service_middleware", self.cluster_id())
    }
    /// Returns the replication mode key, preserving the mode verbatim.
    pub fn replication_mode_path(&self, mode: &str) -> String {
        format!("/pd/{}/replication_mode/{mode}", self.cluster_id())
    }
    /// Returns the store leader weight key with a 20-digit store ID.
    pub fn store_leader_weight_path(&self, id: u64) -> String {
        format!(
            "/pd/{}/schedule/store_weight/{id:020}/leader",
            self.cluster_id()
        )
    }
    /// Returns the store region weight key with a 20-digit store ID.
    pub fn store_region_weight_path(&self, id: u64) -> String {
        format!(
            "/pd/{}/schedule/store_weight/{id:020}/region",
            self.cluster_id()
        )
    }
    /// Returns the member deployment path key with an unpadded ID.
    pub fn member_binary_deploy_path(&self, id: u64) -> String {
        format!("/pd/{}/member/{id}/deploy_path", self.cluster_id())
    }
    /// Returns the member Git hash key.
    pub fn member_git_hash_path(&self, id: u64) -> String {
        format!("/pd/{}/member/{id}/git_hash", self.cluster_id())
    }
    /// Returns the member binary version key.
    pub fn member_binary_version_path(&self, id: u64) -> String {
        format!("/pd/{}/member/{id}/binary_version", self.cluster_id())
    }
    /// Returns the member leader priority key.
    pub fn member_leader_priority_path(&self, id: u64) -> String {
        format!("/pd/{}/member/{id}/leader_priority", self.cluster_id())
    }
    /// Returns the PD leader key for absent/empty service parameters, otherwise
    /// the service primary key. Names are not cleaned or validated.
    pub fn election_path(&self, param: Option<&MsParam>) -> String {
        match param {
            None => format!("/pd/{}/leader", self.cluster_id()),
            Some(p) if p.service_name.is_empty() => format!("/pd/{}/leader", self.cluster_id()),
            Some(p) => self.primary_path(p),
        }
    }
    fn primary_path(&self, p: &MsParam) -> String {
        let cluster = self.cluster_id();
        if p.service_name == "tso" {
            if p.group_id == 0 {
                format!("/ms/{cluster}/tso/00000/primary")
            } else {
                format!(
                    "/ms/{cluster}/tso/keyspace_groups/election/{:05}/primary",
                    p.group_id
                )
            }
        } else {
            format!("/ms/{cluster}/{}/primary", p.service_name)
        }
    }
    /// Returns the expected primary key. Unlike election_path, an empty service
    /// name stays empty; a required reference excludes Go's nil-pointer panic.
    pub fn expected_primary_path(&self, param: &MsParam) -> String {
        format!("{}/expected_primary", self.primary_path(param))
    }
}
