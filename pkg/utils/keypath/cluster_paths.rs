// Copyright 2026 AsterSQL.
//! Cluster and metadata keys corresponding to Go absolute_key_path.go.

/// The cluster ID key is shared across cluster contexts.
pub const CLUSTER_ID_PATH: &str = "/pd/cluster_id";

/// Immutable cluster context; callers supply the ID rather than global state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClusterPaths {
    cluster_id: u64,
}

/// Failure categories from Go strconv.ParseUint for a store ID suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreIdError {
    /// Empty suffix or a non-ASCII decimal digit before overflow.
    InvalidSyntax,
    /// Decimal accumulation exceeded u64 before any later invalid character.
    OutOfRange,
}

impl std::fmt::Display for StoreIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSyntax => "invalid syntax",
            Self::OutOfRange => "value out of range",
        })
    }
}
impl std::error::Error for StoreIdError {}

impl ClusterPaths {
    /// Creates an independent context, including the valid zero cluster ID.
    pub const fn new(cluster_id: u64) -> Self {
        Self { cluster_id }
    }
    /// Returns the explicitly supplied cluster ID.
    pub const fn cluster_id(&self) -> u64 {
        self.cluster_id
    }
    /// Returns the cluster metadata key.
    pub fn cluster_path(&self) -> String {
        format!("/pd/{}/raft", self.cluster_id)
    }
    /// Returns the cluster bootstrap timestamp key.
    pub fn cluster_bootstrap_time_path(&self) -> String {
        format!("{}/status/raft_bootstrap_time", self.cluster_path())
    }
    /// Returns the global ID allocator key within this cluster.
    pub fn alloc_id_path(&self) -> String {
        format!("/pd/{}/alloc_id", self.cluster_id)
    }
    /// Returns the keyspace ID allocator key within this cluster.
    pub fn keyspace_alloc_id_path(&self) -> String {
        format!("/pd/{}/keyspaces/alloc_id", self.cluster_id)
    }
    /// Returns the store metadata prefix, including the trailing slash.
    pub fn store_path_prefix(&self) -> String {
        format!("{}/s/", self.cluster_path())
    }
    /// Returns a store metadata key with a 20-digit decimal ID.
    pub fn store_path(&self, store_id: u64) -> String {
        format!("{}{store_id:020}", self.store_path_prefix())
    }
    /// Returns a region metadata key with a 20-digit decimal ID.
    pub fn region_path(&self, region_id: u64) -> String {
        format!("{}/r/{region_id:020}", self.cluster_path())
    }
    /// Parses Go's permissive store suffix: strip this cluster's prefix once,
    /// then all leading zeroes. Bare decimal IDs are accepted; zero is an error.
    pub fn extract_store_id_from_path(&self, path: &str) -> Result<u64, StoreIdError> {
        let prefix = self.store_path_prefix();
        let digits = path
            .strip_prefix(&prefix)
            .unwrap_or(path)
            .trim_start_matches('0');
        if digits.is_empty() {
            return Err(StoreIdError::InvalidSyntax);
        }
        let mut value = 0_u64;
        for digit in digits.bytes() {
            if !digit.is_ascii_digit() {
                return Err(StoreIdError::InvalidSyntax);
            }
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(u64::from(digit - b'0')))
                .ok_or(StoreIdError::OutOfRange)?;
        }
        Ok(value)
    }
}
