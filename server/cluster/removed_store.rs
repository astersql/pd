// Copyright 2026 AsterSQL.
//! Store heartbeat commit boundary against concurrent store removal.
use super::StoreNotFound;
use astersql_pd_core::{BasicCluster, NodeState, StoreStats};
use astersql_pd_statistics::{HotStat, HotTask};
use astersql_pd_statistics_utils::RWType;
use std::collections::BTreeSet;

pub(super) fn handle_checked(
    cluster: &BasicCluster,
    statistics: Option<&HotStat>,
    stats: &StoreStats,
    now_unix_nanos: i64,
    before_commit: impl FnOnce(),
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

    before_commit();
    // Re-read under the canonical membership lock. Never update the shared raw
    // holder from the preflight snapshot: clones intentionally share that holder.
    let mut state = state;
    cluster.stores.patch_store(store_id, |next| {
        state = next.node_state();
        if next.is_removed() {
            return;
        }
        next.update_raw_stats(stats.clone());
        next.set_last_heartbeat_unix_nanos(now_unix_nanos);
        let Some(hot) = statistics else {
            return;
        };
        // Keep admission inside the same boundary as state mutation. A remove
        // cannot land between the recheck and re-publication of rolling/hot data.
        // Do not call registry-reading health filtering while holding its lock.
        hot.stores_stats.observe(store_id, stats);
        hot.slow_stat
            .observe_slow_store_status(store_id, next.is_slow());
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
            let loads = super::store_heartbeat::peer_loads(peer, interval);
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
    });
    if let Some(hot) = statistics {
        hot.stores_stats
            .filter_unhealthy_store(&cluster.stores, now_unix_nanos as i128);
    }
    Ok(state)
}
