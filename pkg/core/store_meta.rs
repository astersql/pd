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
    /// Last heartbeat as signed Unix nanoseconds, matching Go metadata.
    pub last_heartbeat: i64,
}

/// Store metadata and scheduling weights.
/// The zero value has zero weights; `new` initializes both weights to one.
#[derive(Default)]
pub struct StoreInfo {
    pub(crate) control: [i64; 5],
    pub(crate) last_persist_unix_nanos: Option<i128>,
    pub(crate) last_awaken_unix_nanos: Option<i128>,
    pub(crate) stats: std::sync::Arc<std::sync::RwLock<super::store_clone::SharedStoreStats>>,
    pub(crate) limiter: Option<std::sync::Arc<std::sync::RwLock<super::StoreLimiter>>>,
    pub(crate) meta: Option<std::sync::Arc<StoreMeta>>,
    pub(crate) leader_weight: f64,
    pub(crate) region_weight: f64,
}

impl StoreInfo {
    /// Construct a store with default scheduling weights, even without metadata.
    pub fn new(meta: impl Into<Option<StoreMeta>>) -> Self {
        Self {
            control: [0; 5],
            last_persist_unix_nanos: None,
            last_awaken_unix_nanos: None,
            meta: meta.into().map(std::sync::Arc::new),
            stats: std::sync::Arc::new(std::sync::RwLock::new(
                super::store_clone::SharedStoreStats {
                    average: Some(astersql_pd_movingaverage::Hma::with_size(60.0)),
                    raw: super::StoreStats::default(),
                },
            )),
            limiter: Some(std::sync::Arc::new(std::sync::RwLock::new(
                super::StoreLimiter::V1(astersql_pd_storelimit::StoreRateLimit::new(0.0)),
            ))),
            leader_weight: 1.0,
            region_weight: 1.0,
        }
    }

    pub fn meta(&self) -> Option<&StoreMeta> {
        self.meta.as_deref()
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

impl std::fmt::Debug for StoreInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreInfo")
            .field("meta", &self.meta)
            .field("stats", &self.stats)
            .field("leader_weight", &self.leader_weight)
            .field("region_weight", &self.region_weight)
            .finish_non_exhaustive()
    }
}
