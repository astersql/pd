// Copyright 2026 AsterSQL.
//! Store heartbeat state boundary; protobuf conversion remains in this crate.
use astersql_pd_core::{BasicCluster, NodeState, StoreStats};
use astersql_pd_statistics::HotStat;

/// State errors translated to protocol response headers by the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreHeartbeatError {
    NotBootstrapped,
    StoreNotFound(u64),
    Removed,
    Other(String),
}

/// Host-owned state and clock. Implementations may call `apply_store_heartbeat`.
/// No transport values, scheduler, runtime or worker ownership crosses this API.
#[async_trait::async_trait]
pub trait StoreHeartbeatService: Send + Sync {
    async fn store_heartbeat(&self, stats: StoreStats) -> Result<NodeState, StoreHeartbeatError>;
}

/// Apply a protocol-facing state report through the canonical cluster service.
/// Removed stores are rejected before touching shared statistics, like checkStore.
pub fn apply_store_heartbeat(
    cluster: &mut BasicCluster,
    statistics: Option<&HotStat>,
    stats: &StoreStats,
    now_unix_nanos: i64,
) -> Result<NodeState, StoreHeartbeatError> {
    let store = cluster
        .stores
        .get_store(stats.store_id)
        .ok_or(StoreHeartbeatError::StoreNotFound(stats.store_id))?;
    if store.is_removed() {
        return Err(StoreHeartbeatError::Removed);
    }
    astersql_pd_cluster::handle_store_heartbeat(cluster, statistics, stats, now_unix_nanos)
        .map_err(|error| StoreHeartbeatError::StoreNotFound(error.0))
}
