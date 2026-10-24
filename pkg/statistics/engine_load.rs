// Copyright 2026 AsterSQL.
//! Per-engine store populations and load expectations, without scheduling decisions.

use crate::{
    HistoryLoads, HotPeerStat, Loads, StoreHistoryLoads, StoreKindLoads, StoreLoad, StoreLoadPred,
    StoreSummaryInfo,
};
use astersql_pd_core_constant::ResourceKind;
use astersql_pd_statistics_utils::{DIM_LEN, RWType, StoreLoadKind as S};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

/// Owned store metadata, prediction and denoised hot peers.
#[derive(Clone)]
pub struct StoreLoadDetail {
    pub store_summary_info: StoreSummaryInfo,
    pub load_pred: StoreLoadPred,
    pub hot_peers: Vec<HotPeerStat>,
}
impl StoreLoadDetail {
    /// Compare a normalized standard deviation to a strict threshold.
    pub fn is_uniform(&self, dim: usize, threshold: f64) -> bool {
        self.load_pred.stddev.loads[dim] < threshold
    }
}

/// Incremental totals for one engine's eligible store population.
#[derive(Default)]
pub struct StoreLoadSummary {
    count: usize,
    load_sum: Loads,
    history_sum: HistoryLoads,
    hot_peer_count: f64,
}
impl StoreLoadSummary {
    pub fn add(&mut self, load: &StoreLoad) {
        self.count += 1;
        self.hot_peer_count += load.hot_peer_count;
        for dim in 0..DIM_LEN {
            self.load_sum[dim] += load.loads[dim];
            let values = &load.history_loads[dim];
            if self.history_sum[dim].len() < values.len() {
                self.history_sum[dim].resize(values.len(), 0.0);
            }
            for (sum, value) in self.history_sum[dim].iter_mut().zip(values) {
                *sum += value;
            }
        }
    }
    /// Details must contain the same population of current loads passed to add.
    /// Missing historical slots contribute zero; deviation is divided by the mean.
    pub fn result(&self, details: &[StoreLoadDetail]) -> (StoreLoad, StoreLoad) {
        let mut expect = StoreLoad::default();
        let mut stddev = StoreLoad::default();
        if self.count == 0 {
            return (expect, stddev);
        }
        let n = self.count as f64;
        expect.loads = self.load_sum.map(|v| v / n);
        expect.hot_peer_count = self.hot_peer_count / n;
        expect.history_loads =
            std::array::from_fn(|dim| self.history_sum[dim].iter().map(|v| v / n).collect());
        stddev.hot_peer_count = expect.hot_peer_count;
        if expect.hot_peer_count == 0.0 {
            return (expect, stddev);
        }
        for detail in details {
            for (dim, value) in detail.load_pred.current.loads.iter().enumerate() {
                stddev.loads[dim] += (value - expect.loads[dim]).powi(2);
            }
        }
        for dim in 0..DIM_LEN {
            stddev.loads[dim] = if expect.loads[dim] == 0.0 {
                0.0
            } else {
                (stddev.loads[dim] / n).sqrt() / expect.loads[dim]
            };
        }
        (expect, stddev)
    }
}

#[derive(Clone, Copy)]
enum Collector {
    TiKV,
    TiFlash { trace: bool },
}
impl Collector {
    fn accepts(self, info: &StoreSummaryInfo, kind: ResourceKind) -> bool {
        let store = &info.store_info;
        match (self, kind) {
            (Self::TiKV, ResourceKind::LEADER) => {
                store.is_tikv()
                    && store.allow_leader_transfer_in()
                    && store.allow_leader_transfer_out()
            }
            (Self::TiKV, ResourceKind::REGION) => store.is_tikv(),
            (Self::TiFlash { .. }, ResourceKind::REGION) => store.is_tiflash_write(),
            _ => false,
        }
    }
    fn loads(self, store: &StoreKindLoads, peer: Loads, rw: RWType, kind: ResourceKind) -> Loads {
        let at = |k: S| store[k.as_i32() as usize];
        match (self, rw, kind) {
            (Self::TiKV, RWType::READ, _) => [
                at(S::READ_BYTES),
                at(S::READ_KEYS),
                at(S::READ_QUERY),
                at(S::READ_CPU),
            ],
            (Self::TiKV, RWType::WRITE, ResourceKind::LEADER) => {
                [peer[0], peer[1], at(S::WRITE_QUERY), 0.0]
            }
            (Self::TiKV, RWType::WRITE, ResourceKind::REGION) => {
                [at(S::WRITE_BYTES), at(S::WRITE_KEYS), 0.0, 0.0]
            }
            (Self::TiFlash { trace: true }, RWType::WRITE, ResourceKind::REGION) => [
                at(S::REGIONS_WRITE_BYTES),
                at(S::REGIONS_WRITE_KEYS),
                0.0,
                0.0,
            ],
            (Self::TiFlash { trace: false }, RWType::WRITE, ResourceKind::REGION) => {
                [peer[0], peer[1], 0.0, 0.0]
            }
            _ => [0.0; 4],
        }
    }
}

/// Summarize each engine independently, returning stores in ID order.
/// Read history before adding the current sample; removed stores never recreate
/// history. Pending influence affects future loads only. The host supplies time.
#[allow(clippy::too_many_arguments)]
pub fn summary_stores_load(
    infos: &HashMap<u64, StoreSummaryInfo>,
    loads: &HashMap<u64, StoreKindLoads>,
    mut history: Option<&mut StoreHistoryLoads>,
    peers: &HashMap<u64, Vec<HotPeerStat>>,
    trace: bool,
    rw: RWType,
    kind: ResourceKind,
    now: Duration,
) -> BTreeMap<u64, StoreLoadDetail> {
    let mut result = BTreeMap::new();
    let mut population: Vec<_> = infos.values().collect();
    population.sort_unstable_by_key(|info| info.store_info.id());
    for collector in [Collector::TiKV, Collector::TiFlash { trace }] {
        let mut summary = StoreLoadSummary::default();
        let mut details = Vec::new();
        for &info in &population {
            let id = info.store_info.id();
            let Some(store_loads) = loads.get(&id).filter(|_| collector.accepts(info, kind)) else {
                continue;
            };
            let mut sum = [0.0; 4];
            let mut hot_peers = Vec::new();
            for peer in peers
                .get(&id)
                .into_iter()
                .flatten()
                .filter(|p| kind == ResourceKind::REGION || p.is_leader)
            {
                for (dim, value) in sum.iter_mut().enumerate() {
                    *value += peer.get_load(dim as isize);
                }
                hot_peers.push(peer.clone());
            }
            let current = StoreLoad {
                loads: collector.loads(store_loads, sum, rw, kind),
                hot_peer_count: hot_peers.len() as f64,
                history_loads: history
                    .as_ref()
                    .map(|h| h.get(id, rw, kind))
                    .unwrap_or_default(),
            };
            summary.add(&current);
            if let Some(history) = history.as_mut() {
                history.add_store(&info.store_info, rw, kind, current.loads, now);
            }
            details.push(StoreLoadDetail {
                store_summary_info: info.clone(),
                load_pred: current.to_load_pred(rw, info.pending_sum.as_ref()),
                hot_peers,
            });
        }
        let (expect, stddev) = summary.result(&details);
        for mut detail in details {
            detail.load_pred.expect = expect.clone();
            detail.load_pred.stddev = stddev.clone();
            result.insert(detail.store_summary_info.store_info.id(), detail);
        }
    }
    result
}
