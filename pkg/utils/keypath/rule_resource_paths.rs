// Copyright 2026 AsterSQL.
//! Placement rule, affinity and resource group metadata keys.
use crate::{ClusterPaths, NULL_KEYSPACE_ID};

impl ClusterPaths {
    /// Returns the rules key without cleaning or escaping the supplied name.
    pub fn rule_key_path(&self, name: &str) -> String {
        format!("/pd/{}/rules/{name}", self.cluster_id())
    }
    /// Returns the rule_group key without cleaning or escaping the supplied name.
    pub fn rule_group_id_path(&self, name: &str) -> String {
        format!("/pd/{}/rule_group/{name}", self.cluster_id())
    }
    /// Returns the region_label key without cleaning or escaping the supplied name.
    pub fn region_label_key_path(&self, name: &str) -> String {
        format!("/pd/{}/region_label/{name}", self.cluster_id())
    }
    /// Returns the affinity_groups key without cleaning or escaping the supplied name.
    pub fn affinity_group_path(&self, name: &str) -> String {
        format!("/pd/{}/affinity_groups/{name}", self.cluster_id())
    }
    /// Returns the prefix including its trailing slash.
    pub fn rules_path_prefix(&self) -> String {
        self.rule_key_path("")
    }
    /// Returns the prefix including its trailing slash.
    pub fn rule_group_path_prefix(&self) -> String {
        self.rule_group_id_path("")
    }
    /// Returns the prefix including its trailing slash.
    pub fn region_label_path_prefix(&self) -> String {
        self.region_label_key_path("")
    }
    /// Returns the prefix including its trailing slash.
    pub fn affinity_groups_prefix(&self) -> String {
        self.affinity_group_path("")
    }
    /// Returns the common watch prefix for rules and rule groups.
    pub fn rule_common_path_prefix(&self) -> String {
        format!("/pd/{}/rule", self.cluster_id())
    }
}
/// Returns the `resource_group/controller` metadata path.
pub const fn controller_config_path() -> &'static str {
    "resource_group/controller"
}
/// Returns the `resource_group/` metadata path.
pub const fn resource_group_prefix() -> &'static str {
    "resource_group/"
}
/// Returns the `resource_group/settings/` metadata path.
pub const fn resource_group_setting_prefix() -> &'static str {
    "resource_group/settings/"
}
/// Returns the `resource_group/states/` metadata path.
pub const fn resource_group_state_prefix() -> &'static str {
    "resource_group/states/"
}
/// Returns the `resource_group/keyspace/settings/` metadata path.
pub const fn keyspace_resource_group_setting_prefix() -> &'static str {
    "resource_group/keyspace/settings/"
}
/// Returns the `resource_group/keyspace/states/` metadata path.
pub const fn keyspace_resource_group_state_prefix() -> &'static str {
    "resource_group/keyspace/states/"
}
/// Returns the `resource_group/keyspace/service_limits/` metadata path.
pub const fn keyspace_service_limit_prefix() -> &'static str {
    "resource_group/keyspace/service_limits/"
}
/// Returns a keyspace resource group setting key; Null selects the legacy schema.
pub fn keyspace_resource_group_setting_path(id: u32, name: &str) -> String {
    if id == NULL_KEYSPACE_ID {
        format!("{}{name}", resource_group_setting_prefix())
    } else {
        format!("{}{id}/{name}", keyspace_resource_group_setting_prefix())
    }
}
/// Returns a keyspace resource group state key; Null selects the legacy schema.
pub fn keyspace_resource_group_state_path(id: u32, name: &str) -> String {
    if id == NULL_KEYSPACE_ID {
        format!("{}{name}", resource_group_state_prefix())
    } else {
        format!("{}{id}/{name}", keyspace_resource_group_state_prefix())
    }
}
/// Returns the service limit key (including Null as a decimal ID).
pub fn keyspace_service_limit_path(id: u32) -> String {
    format!("{}{id}", keyspace_service_limit_prefix())
}

/// Invalid relative resource group suffix, preserving Go's error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceGroupPathError {
    /// No slash separator was present.
    InvalidPath(String),
    /// The first segment was not an unsigned decimal u32.
    InvalidKeyspaceId(String),
}
impl std::fmt::Display for ResourceGroupPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPath(path) => {
                write!(f, "invalid keyspace resource group setting path: {path}")
            }
            Self::InvalidKeyspaceId(id) => write!(f, "invalid keyspace ID str: {id}"),
        }
    }
}
impl std::error::Error for ResourceGroupPathError {}

/// Parses a relative `ID/name` suffix, retaining nested slashes and empty names.
pub fn parse_keyspace_resource_group_path(
    path: &str,
) -> Result<(u32, &str), ResourceGroupPathError> {
    let (id, name) = path
        .split_once('/')
        .ok_or_else(|| ResourceGroupPathError::InvalidPath(path.to_owned()))?;
    let invalid = || ResourceGroupPathError::InvalidKeyspaceId(id.to_owned());
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    Ok((id.parse::<u32>().map_err(|_| invalid())?, name))
}
