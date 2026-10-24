// Copyright 2026 AsterSQL.
//! Store reports compose canonical core state and statistics without scheduling.
use std::collections::BTreeSet;

use astersql_pd_core::{BasicCluster, NodeState, PeerStat, StoreStats};
use astersql_pd_statistics::{HotStat, HotTask, region_read_cpu_usage};
use astersql_pd_statistics_utils::RWType;

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
    let store_id = stats.store_id;
    let store = cluster
        .stores
        .get_store(store_id)
        .ok_or(StoreNotFound(store_id))?;
    let state = store.node_state();
    if store.is_removed() {
        if let Some(hot) = statistics {
            hot.stores_stats
                .filter_unhealthy_store(&cluster.stores, now_unix_nanos as i128);
        }
        return Ok(state);
    }

    let mut next = store.as_ref().clone();
    next.update_raw_stats(stats.clone());
    next.set_last_heartbeat_unix_nanos(now_unix_nanos);
    let slow = next.is_slow();
    cluster.stores.put_store(std::sync::Arc::new(next));

    let Some(hot) = statistics else {
        return Ok(state);
    };
    hot.stores_stats.observe(store_id, stats);
    hot.stores_stats
        .filter_unhealthy_store(&cluster.stores, now_unix_nanos as i128);
    hot.slow_stat.observe_slow_store_status(store_id, slow);
    let interval = stats
        .interval
        .as_ref()
        .map_or(0, |i| i.end_timestamp.wrapping_sub(i.start_timestamp));
    let mut reported = BTreeSet::new();
    for peer in &stats.peer_stats {
        reported.insert(peer.region_id);
        let Some(region) = cluster.regions.get_region(peer.region_id) else {
            continue;
        };
        if !region.meta().peers.iter().any(|p| p.store_id == store_id) {
            continue;
        }
        let loads = peer_loads(peer, interval);
        // Queue rejection deliberately does not reject the heartbeat itself.
        let _ = hot.hot_cache.submit(
            RWType::READ,
            HotTask::read_peer(&region, store_id, loads, interval),
            now_unix_nanos,
        );
    }
    let _ = hot.hot_cache.submit(
        RWType::READ,
        HotTask::Cold {
            store: store_id,
            reported,
            interval,
        },
        now_unix_nanos,
    );
    Ok(state)
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
