// Copyright 2026 AsterSQL.
//! Owned query projections of hotspot loads. No transport or clock ownership.

use crate::{
    HotPeerStat, StoreKindLoads, StoreLoadDetail, summary_store_infos, summary_stores_load,
};
use astersql_pd_core::StoreInfo;
use astersql_pd_core_constant::ResourceKind;
use astersql_pd_statistics_utils::RWType;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

/// Per-store query results, keyed by store ID.
pub type StoreHotPeersStat = BTreeMap<u64, HotPeersStat>;

/// Leader and all-peer projections; learners belong to the all-peer population.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct StoreHotPeersInfos {
    pub as_peer: StoreHotPeersStat,
    pub as_leader: StoreHotPeersStat,
}

/// Store rates and the sum of the returned hotspot entries, in bytes/keys/queries/CPU per second.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct HotPeersStat {
    #[serde(rename = "store_bytes")]
    pub store_byte_rate: f64,
    #[serde(rename = "store_keys")]
    pub store_key_rate: f64,
    #[serde(rename = "store_query")]
    pub store_query_rate: f64,
    #[serde(rename = "store_cpu")]
    pub store_cpu_rate: f64,
    #[serde(rename = "total_flow_bytes")]
    pub total_bytes_rate: f64,
    #[serde(rename = "total_flow_keys")]
    pub total_keys_rate: f64,
    #[serde(rename = "total_flow_query")]
    pub total_query_rate: f64,
    #[serde(rename = "total_flow_cpu")]
    pub total_cpu_rate: f64,
    #[serde(rename = "regions_count")]
    pub count: usize,
    #[serde(rename = "statistics")]
    pub stats: Option<Vec<HotPeerStatShow>>,
}

/// Query entry. The adapter may enrich learner and RFC3339 update-time metadata.
#[derive(Debug, PartialEq, Serialize)]
pub struct HotPeerStatShow {
    pub store_id: u64,
    pub stores: Option<Vec<u64>>,
    pub is_leader: bool,
    pub is_learner: bool,
    pub region_id: u64,
    pub hot_degree: i64,
    #[serde(rename = "flow_bytes")]
    pub byte_rate: f64,
    #[serde(rename = "flow_keys")]
    pub key_rate: f64,
    #[serde(rename = "flow_query")]
    pub query_rate: f64,
    #[serde(rename = "flow_cpu")]
    pub cpu_rate: f64,
    pub anti_count: i64,
    pub last_update_time: String,
}

impl HotPeerStatShow {
    /// Project rounded loads; learner/time metadata belongs to the caller.
    pub fn from_peer(peer: &HotPeerStat) -> Self {
        Self {
            store_id: peer.store_id,
            stores: if peer.stores.is_empty() {
                None
            } else {
                Some(peer.stores.clone())
            },
            is_leader: peer.is_leader,
            is_learner: false,
            region_id: peer.region_id,
            hot_degree: peer.hot_degree,
            byte_rate: peer.get_load(0),
            key_rate: peer.get_load(1),
            query_rate: peer.get_load(2),
            cpu_rate: peer.get_load(3),
            anti_count: peer.anti_count,
            last_update_time: "0001-01-01T00:00:00Z".into(),
        }
    }
}

impl HotPeersStat {
    fn add_peer_loads(&mut self, peer: &HotPeerStat) {
        self.total_bytes_rate += peer.get_load(0);
        self.total_keys_rate += peer.get_load(1);
        self.total_query_rate += peer.get_load(2);
        self.total_cpu_rate += peer.get_load(3);
        self.count += 1;
    }
}

impl StoreLoadDetail {
    /// Snapshot positive-degree peers, preserving order and current store rates.
    pub fn to_hot_peers_stat(&self) -> HotPeersStat {
        let loads = self.load_pred.current.loads;
        let mut result = HotPeersStat {
            store_byte_rate: loads[0],
            store_key_rate: loads[1],
            store_query_rate: loads[2],
            store_cpu_rate: loads[3],
            ..Default::default()
        };
        let mut stats = Vec::with_capacity(self.hot_peers.len());
        for peer in self.hot_peers.iter().filter(|peer| peer.hot_degree > 0) {
            result.add_peer_loads(peer);
            stats.push(HotPeerStatShow::from_peer(peer));
        }
        result.stats = Some(stats);
        result
    }
}

/// Lightweight totals include every supplied peer, even non-positive degrees.
/// Missing map keys are omitted; present empty populations retain zero totals.
pub fn collect_hot_peer_infos(
    stores: &[StoreInfo],
    peers: &HashMap<u64, Vec<HotPeerStat>>,
) -> StoreHotPeersInfos {
    let mut result = StoreHotPeersInfos::default();
    for store in stores {
        let id = store.id();
        let Some(peers) = peers.get(&id) else {
            continue;
        };
        let mut as_peer = HotPeersStat::default();
        let mut as_leader = HotPeersStat::default();
        for peer in peers {
            as_peer.add_peer_loads(peer);
            if peer.is_leader {
                as_leader.add_peer_loads(peer);
            }
        }
        result.as_peer.insert(id, as_peer);
        result.as_leader.insert(id, as_leader);
    }
    result
}

/// Project the existing per-engine aggregation without pending/history state.
/// Learner and last-update metadata are enriched by the caller, as in Go.
pub fn get_hot_status(
    stores: &[StoreInfo],
    loads: &HashMap<u64, StoreKindLoads>,
    peers: &HashMap<u64, Vec<HotPeerStat>>,
    rw: RWType,
    trace: bool,
) -> StoreHotPeersInfos {
    let infos = summary_store_infos(stores);
    let collect = |kind| {
        summary_stores_load(&infos, loads, None, peers, trace, rw, kind, Duration::ZERO)
            .into_iter()
            .map(|(id, detail)| (id, detail.to_hot_peers_stat()))
            .collect()
    };
    StoreHotPeersInfos {
        as_peer: collect(ResourceKind::REGION),
        as_leader: collect(ResourceKind::LEADER),
    }
}
