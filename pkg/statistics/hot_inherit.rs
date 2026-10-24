// Copyright 2026 AsterSQL.
use std::collections::BTreeSet;

use astersql_pd_core::RegionInfo;
use astersql_pd_statistics_utils::ActionType;

use crate::{HotPeerCache, HotPeerStat};

impl HotPeerCache {
    /// Finds an inheritable peer, or preserves the final direct lookup result.
    /// Cached stores precede unique region stores. Go's old-store map has no
    /// defined order; this implementation uses ascending store IDs.
    pub fn find_old_hot_peer_for_inheritance(
        &self,
        region: &RegionInfo,
    ) -> (Option<&HotPeerStat>, bool) {
        let old_stores: BTreeSet<_> = self
            .peers
            .keys()
            .filter(|(id, _)| *id == region.meta().id)
            .map(|(_, store)| *store)
            .collect();
        let mut seen = old_stores.clone();
        let mut last = None;
        for store in old_stores.into_iter().chain(
            region
                .meta()
                .peers
                .iter()
                .filter_map(|peer| seen.insert(peer.store_id).then_some(peer.store_id)),
        ) {
            last = self.get(region.meta().id, store);
            if last.is_some_and(|item| item.allow_inherited) {
                return (last, true);
            }
        }
        (last, false)
    }

    /// Returns removal actions for stores absent from the reported replica set.
    /// Apply these after checking flow so removed replicas can donate history.
    pub fn collect_expired_items(&self, region: &RegionInfo) -> Vec<HotPeerStat> {
        self.peers
            .values()
            .filter(|item| {
                item.region_id == region.meta().id
                    && !region
                        .meta()
                        .peers
                        .iter()
                        .any(|peer| peer.store_id == item.store_id)
            })
            .map(|item| {
                let mut item = item.clone();
                item.action_type = ActionType::REMOVE;
                item
            })
            .collect()
    }

    pub(crate) fn just_transfer_leader(&self, region: &RegionInfo, old: &HotPeerStat) -> bool {
        let leader = region.leader().map_or(0, |p| p.store_id);
        if old.is_leader {
            return old.store_id != leader;
        }
        self.peers
            .values()
            .find(|item| item.region_id == region.meta().id && item.is_leader)
            .is_some_and(|item| item.store_id != leader)
    }
}
