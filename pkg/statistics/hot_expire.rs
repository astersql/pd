// Copyright 2026 AsterSQL.
use std::collections::{BTreeMap, BTreeSet};

use astersql_pd_statistics_utils::{ActionType, RWType};

use crate::{HotPeerCache, HotPeerStat};

pub(crate) struct HotExpiry {
    pub(crate) updated: BTreeMap<(u64, u64), i64>,
    ttl: u64,
    last_gc: i64,
}

impl HotExpiry {
    pub(crate) fn new(kind: RWType) -> Self {
        Self {
            updated: BTreeMap::new(),
            ttl: 3 * kind.report_interval() * 1_000_000_000,
            last_gc: 0,
        }
    }
}

impl HotPeerCache {
    /// Returns cold actions for cached peers omitted by one store's heartbeat.
    /// Reported IDs are store-local. Checking does not mutate cached windows;
    /// the host applies actions with update_stat_at before the next report.
    pub fn check_cold_peer(
        &self,
        store: u64,
        reported: &BTreeSet<u64>,
        interval: u64,
        now: i64,
    ) -> Vec<HotPeerStat> {
        if self.denoising && interval < 3 {
            return vec![];
        }
        let mut result = Vec::new();
        for old in self
            .peers
            .values()
            .filter(|old| old.store_id == store && !reported.contains(&old.region_id))
        {
            let mut item = HotPeerStat {
                store_id: store,
                region_id: old.region_id,
                loads: vec![0.; old.loads.len()],
                is_leader: old.is_leader,
                action_type: ActionType::UPDATE,
                in_cold: true,
                stores: old.stores.clone(),
                ..Default::default()
            };
            let thresholds = self.calc_hot_thresholds(store, now);
            // Preserve Go's existing cold-report delta layout: thresholds occupy
            // the first four RegionStatKind slots, even in the write direction.
            let mut delta = [0.; 8];
            for (dim, threshold) in thresholds.iter().enumerate() {
                delta[dim] = threshold * interval as f64;
            }
            self.update_hot_peer_stat(
                &mut item,
                old,
                &delta,
                (interval as i64).wrapping_mul(1_000_000_000),
                &thresholds,
                false,
                None,
            );
            result.push(item);
        }
        result
    }

    /// Changes the TTL and GC interval in nanoseconds, retaining last-update times.
    /// The default is three directional reporting intervals. Zero expires at GC.
    pub fn set_peer_ttl(&mut self, nanos: u64) {
        self.expiry.ttl = nanos;
    }

    /// Runs throttled GC at an explicit host clock. The host provides all live
    /// stores, excluding tombstones. Clock rollback postpones GC. Deadline equality
    /// expires a peer; Add/Update refreshes its timestamp, Remove forgets it.
    pub fn gc(&mut self, now: i64, active_stores: &BTreeSet<u64>) {
        if now as i128 - (self.expiry.last_gc as i128) < self.expiry.ttl as i128 {
            return;
        }
        self.expiry.last_gc = now;
        let expired: Vec<_> = self
            .peers
            .values()
            .filter(|peer| {
                !active_stores.contains(&peer.store_id)
                    || self
                        .expiry
                        .updated
                        .get(&(peer.region_id, peer.store_id))
                        .is_some_and(|updated| {
                            now as i128 - *updated as i128 >= self.expiry.ttl as i128
                        })
            })
            .map(|peer| {
                let mut item = peer.clone();
                item.action_type = ActionType::REMOVE;
                item
            })
            .collect();
        for item in expired {
            self.update_stat_at(item, now);
        }
        // A store can acquire a threshold snapshot without ever becoming hot.
        self.hot_thresholds.retain_stores(active_stores);
    }

    /// Removes every cached replica of a disappeared region, idempotently.
    pub fn remove_region(&mut self, region: u64) {
        let items: Vec<_> = self
            .peers
            .range((region, 0)..=(region, u64::MAX))
            .map(|(_, peer)| {
                let mut item = peer.clone();
                item.action_type = ActionType::REMOVE;
                item
            })
            .collect();
        for item in items {
            self.update_stat(item);
        }
    }

    /// Returns store membership from the canonical peer index, without tombstones.
    pub fn stores_of_region(&self, region: u64) -> BTreeSet<u64> {
        self.peers
            .range((region, 0)..=(region, u64::MAX))
            .map(|((_, store), _)| *store)
            .collect()
    }

    /// Returns region membership from the canonical peer index, without tombstones.
    pub fn regions_of_store(&self, store: u64) -> BTreeSet<u64> {
        self.peers
            .keys()
            .filter_map(|(region, id)| (*id == store).then_some(*region))
            .collect()
    }
}
