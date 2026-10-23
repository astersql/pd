// Copyright 2026 AsterSQL.
//! Keyspace and timestamp keys corresponding to Go absolute_key_path.go.
use crate::ClusterPaths;

impl ClusterPaths {
    /// Returns the keyspace metadata prefix, including its trailing slash.
    pub fn keyspace_meta_prefix(&self) -> String {
        format!("/pd/{}/keyspaces/meta/", self.cluster_id())
    }
    /// Returns a metadata key, padding IDs to a minimum of eight digits.
    pub fn keyspace_meta_path(&self, space_id: u32) -> String {
        format!("{}{space_id:08}", self.keyspace_meta_prefix())
    }
    /// Returns a name-to-ID key without cleaning or escaping the name.
    pub fn keyspace_id_path(&self, name: &str) -> String {
        format!("/pd/{}/keyspaces/id/{name}", self.cluster_id())
    }
    /// Returns the TSO membership prefix, including its trailing slash.
    pub fn keyspace_group_id_prefix(&self) -> String {
        format!("/pd/{}/tso/keyspace_groups/membership/", self.cluster_id())
    }
    /// Returns a membership key, padding group IDs to at least five digits.
    pub fn keyspace_group_id_path(&self, group_id: u32) -> String {
        format!("{}{group_id:05}", self.keyspace_group_id_prefix())
    }
    /// Returns the persisted timestamp key for the supplied keyspace group.
    pub fn timestamp_path(&self, group_id: u32) -> String {
        if group_id == 0 {
            format!("/pd/{}/timestamp", self.cluster_id())
        } else {
            format!("/ms/{}/tso/{group_id:05}/gta/timestamp", self.cluster_id())
        }
    }
    /// Returns the minimum resolved timestamp key.
    pub fn min_resolved_ts_path(&self) -> String {
        format!("{}/min_resolved_ts", self.cluster_path())
    }
    /// Returns the external timestamp key.
    pub fn external_timestamp_path(&self) -> String {
        format!("{}/external_timestamp", self.cluster_path())
    }
    /// Returns a meta-service status key without normalizing the group name.
    pub fn meta_service_group_status_path(&self, group_id: &str) -> String {
        format!(
            "/pd/{}/meta_service_groups/{group_id}/status",
            self.cluster_id()
        )
    }
}

/// Returns the capture of Go's `tso/keyspace_groups/membership/(\d{5})$`.
/// The prefix is unanchored; the final five digits must be ASCII. This matcher
/// deliberately does not validate a cluster prefix or accept wider group IDs.
pub fn keyspace_group_id_from_path(path: &str) -> Option<&str> {
    let (_, digits) = path.rsplit_once("tso/keyspace_groups/membership/")?;
    (digits.len() == 5 && digits.bytes().all(|byte| byte.is_ascii_digit())).then_some(digits)
}
