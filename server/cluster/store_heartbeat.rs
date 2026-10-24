// Copyright 2026 AsterSQL.
//! Store reports compose canonical core state and statistics without scheduling.
use astersql_pd_core::{BasicCluster, NodeState, PeerStat, StoreStats};
use astersql_pd_statistics::{HotStat, region_read_cpu_usage};

/// Unknown store; preserves the Go heartbeat handler's error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreNotFound(pub u64);
impl std::fmt::Display for StoreNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "store {} not found", self.0)
    }
}
impl std::error::Error for StoreNotFound {}

/// Applies an existing store's report using a host-supplied Unix nanosecond clock.
///
/// Exclusive cluster access serializes reports with region and store mutations.
/// `None` statistics selects the independent scheduling service path. Hot tasks
/// use best-effort nonblocking admission, as in Go; success confirms core state,
/// not application of every queued task. The host owns HotStat shutdown/join.
/// The returned node state supplies the transport response for an active store;
/// a Removed store is a successful no-op and its returned state is informational.
/// Persistence, limiter configuration and scheduling feedback belong to the host.
pub fn handle_store_heartbeat(
    cluster: &mut BasicCluster,
    statistics: Option<&HotStat>,
    stats: &StoreStats,
    now_unix_nanos: i64,
) -> Result<NodeState, StoreNotFound> {
    super::removed_store::handle_checked(cluster, statistics, stats, now_unix_nanos, || {})
}

/// Capture cumulative read inputs before the canonical cache divides by seconds.
pub(super) fn peer_loads(peer: &PeerStat, interval: u64) -> [f64; 8] {
    let queries = peer.query_stats.as_ref().map_or(0, |q| q.read_query_num());
    [
        peer.read_bytes as f64,
        peer.read_keys as f64,
        queries as f64,
        0.,
        0.,
        0.,
        region_read_cpu_usage(Some(peer)) * interval as f64,
        0.,
    ]
}
