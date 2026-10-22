// Copyright 2026 AsterSQL.
//! Owned store identity and constructor defaults, independent of protobuf.

/// Store metadata preserves values without validating addresses or versions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreMeta {
    pub labels: Vec<super::StoreLabel>,
    pub state: super::StoreState,
    pub node_state: super::NodeState,
    pub physically_destroyed: bool,
    pub id: u64,
    pub address: String,
    pub status_address: String,
    pub version: String,
    pub deploy_path: String,
    /// Signed seconds since the Unix epoch, matching Go's Store timestamp.
    pub start_timestamp: i64,
}

/// Store metadata and scheduling weights.
/// The zero value has zero weights; `new` initializes both weights to one.
#[derive(Debug, Default)]
pub struct StoreInfo {
    pub(crate) raw_stats: super::StoreStats,
    pub(crate) meta: Option<StoreMeta>,
    leader_weight: f64,
    region_weight: f64,
}

impl StoreInfo {
    /// Construct a store with default scheduling weights, even without metadata.
    pub fn new(meta: impl Into<Option<StoreMeta>>) -> Self {
        Self {
            meta: meta.into(),
            raw_stats: super::StoreStats::default(),
            leader_weight: 1.0,
            region_weight: 1.0,
        }
    }

    pub fn meta(&self) -> Option<&StoreMeta> {
        self.meta.as_ref()
    }

    pub fn id(&self) -> u64 {
        self.meta.as_ref().map_or(0, |meta| meta.id)
    }

    pub fn address(&self) -> &str {
        self.meta.as_ref().map_or("", |meta| meta.address.as_str())
    }

    pub fn status_address(&self) -> &str {
        self.meta
            .as_ref()
            .map_or("", |meta| meta.status_address.as_str())
    }

    pub fn version(&self) -> &str {
        self.meta.as_ref().map_or("", |meta| meta.version.as_str())
    }

    pub fn deploy_path(&self) -> &str {
        self.meta
            .as_ref()
            .map_or("", |meta| meta.deploy_path.as_str())
    }

    /// Start time as signed Unix seconds. Zero denotes 1970-01-01T00:00:00Z.
    pub fn start_time_unix_seconds(&self) -> i64 {
        self.meta.as_ref().map_or(0, |meta| meta.start_timestamp)
    }

    pub fn leader_weight(&self) -> f64 {
        self.leader_weight
    }

    pub fn region_weight(&self) -> f64 {
        self.region_weight
    }
}
