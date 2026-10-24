// Copyright 2026 AsterSQL.

use astersql_pd_core::{Peer, RegionInfo, is_empty_region};
use astersql_pd_statistics_utils::{DIM_LEN, RWType};
use std::collections::BTreeMap;

/// Owned, already denoised and rounded HotPeer GetLoad values, per second.
/// The host captures these from its hot peer statistics; no heartbeat totals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HotPeerSnapshot {
    pub loads: [f64; DIM_LEN],
}

/// Read-only access to hot peer snapshots needed by region aggregation.
pub trait RegionStatInformer {
    fn get_hot_peer_stat(
        &self,
        rw: RWType,
        region_id: u64,
        store_id: u64,
    ) -> Option<HotPeerSnapshot>;
}

/// A predicate on peer membership. All supplied predicates must accept a peer.
pub type GetRegionStatsOption<'a> = &'a dyn Fn(&Peer) -> bool;

/// Select stores by membership; engine strings do not change peer sizes.
pub fn with_store_map_option(stores: &BTreeMap<u64, String>) -> impl Fn(&Peer) -> bool + '_ {
    move |peer| stores.contains_key(&peer.store_id)
}

/// Region totals and per-store distributions. Sizes are MiB; loads are per second.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionStats {
    pub count: i64,
    pub empty_count: i64,
    pub storage_size: i64,
    pub user_storage_size: i64,
    pub user_ia_storage_size: i64,
    pub user_columnar_storage_size: i64,
    pub storage_keys: i64,
    pub store_leader_count: BTreeMap<u64, i64>,
    pub store_peer_count: BTreeMap<u64, i64>,
    pub store_leader_size: BTreeMap<u64, i64>,
    pub store_leader_keys: BTreeMap<u64, i64>,
    pub store_peer_size: BTreeMap<u64, i64>,
    pub store_peer_keys: BTreeMap<u64, i64>,
    pub store_write_bytes: BTreeMap<u64, u64>,
    pub store_write_keys: BTreeMap<u64, u64>,
    pub store_write_query: BTreeMap<u64, u64>,
    pub store_leader_read_bytes: BTreeMap<u64, u64>,
    pub store_leader_read_keys: BTreeMap<u64, u64>,
    pub store_leader_read_query: BTreeMap<u64, u64>,
    pub store_peer_read_bytes: BTreeMap<u64, u64>,
    pub store_peer_read_keys: BTreeMap<u64, u64>,
    pub store_peer_read_query: BTreeMap<u64, u64>,
    /// Caller-owned engine annotations; observing regions leaves these untouched.
    pub store_engine: BTreeMap<u64, String>,
}

/// Aggregate the supplied regions without retaining references to them.
pub fn get_region_stats(
    regions: &[RegionInfo],
    cluster: Option<&dyn RegionStatInformer>,
    opts: &[GetRegionStatsOption<'_>],
) -> RegionStats {
    let mut stats = RegionStats::default();
    for region in regions {
        stats.observe(region, cluster, opts);
    }
    stats
}
impl RegionStats {
    /// Add one region, filtering only its store distributions and hot loads.
    pub fn observe(
        &mut self,
        region: &RegionInfo,
        cluster: Option<&dyn RegionStatInformer>,
        opts: &[GetRegionStatsOption<'_>],
    ) {
        let r = region.stats();
        self.count = self.count.wrapping_add(1);
        if is_empty_region(r.approximate_size, r.approximate_keys) {
            self.empty_count = self.empty_count.wrapping_add(1);
        }
        self.storage_size = self.storage_size.wrapping_add(r.approximate_size);
        self.user_storage_size = self.user_storage_size.wrapping_add(r.approximate_kv_size);
        self.user_ia_storage_size = self
            .user_ia_storage_size
            .wrapping_add(r.approximate_ia_kv_size.min(r.approximate_kv_size));
        self.user_columnar_storage_size = self
            .user_columnar_storage_size
            .wrapping_add(r.approximate_columnar_kv_size);
        self.storage_keys = self.storage_keys.wrapping_add(r.approximate_keys);
        let accepts = |peer: &Peer| opts.iter().all(|opt| opt(peer));
        let id = region.meta().id;
        if let Some(leader) = region.leader().filter(|p| accepts(p)) {
            let store = leader.store_id;
            add_signed(&mut self.store_leader_count, store, 1);
            add_signed(&mut self.store_leader_size, store, r.approximate_size);
            add_signed(&mut self.store_leader_keys, store, r.approximate_keys);
            if let Some(hot) = cluster.and_then(|c| c.get_hot_peer_stat(RWType::READ, id, store)) {
                add_loads(
                    &mut self.store_leader_read_bytes,
                    &mut self.store_leader_read_keys,
                    &mut self.store_leader_read_query,
                    store,
                    hot,
                );
            }
        }
        for peer in region.meta().peers.iter().filter(|p| accepts(p)) {
            let store = peer.store_id;
            add_signed(&mut self.store_peer_count, store, 1);
            add_signed(
                &mut self.store_peer_size,
                store,
                region.store_peer_approximate_size(store),
            );
            add_signed(
                &mut self.store_peer_keys,
                store,
                region.store_peer_approximate_keys(store),
            );
            if let Some(cluster) = cluster {
                if let Some(hot) = cluster.get_hot_peer_stat(RWType::READ, id, store) {
                    add_loads(
                        &mut self.store_peer_read_bytes,
                        &mut self.store_peer_read_keys,
                        &mut self.store_peer_read_query,
                        store,
                        hot,
                    );
                }
                if let Some(hot) = cluster.get_hot_peer_stat(RWType::WRITE, id, store) {
                    add_loads(
                        &mut self.store_write_bytes,
                        &mut self.store_write_keys,
                        &mut self.store_write_query,
                        store,
                        hot,
                    );
                }
            }
        }
    }
}
fn add_signed(map: &mut BTreeMap<u64, i64>, store: u64, value: i64) {
    let entry = map.entry(store).or_default();
    *entry = entry.wrapping_add(value);
}
fn add_loads(
    bytes: &mut BTreeMap<u64, u64>,
    keys: &mut BTreeMap<u64, u64>,
    query: &mut BTreeMap<u64, u64>,
    store: u64,
    hot: HotPeerSnapshot,
) {
    for (map, value) in [bytes, keys, query].into_iter().zip(hot.loads) {
        let entry = map.entry(store).or_default();
        *entry = entry.wrapping_add(value as u64);
    }
}
