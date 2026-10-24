// Copyright 2026 AsterSQL.

use std::collections::BTreeMap;

use astersql_pd_core::RegionInfo;
use astersql_pd_statistics_utils::{ActionType, DIM_LEN, MIN_HOT_THRESHOLDS, RWType};

use crate::{DimStat, HotPeerStat};

const SECOND: i64 = 1_000_000_000;

/// Synchronous directional peer lifecycle. The host supplies per-store thresholds
/// and applies returned actions in report order. Read and write use separate caches.
/// TopN thresholds, membership inheritance, TTL and asynchronous work are separate
/// policies; this type owns only the direct-report state machine.
pub struct HotPeerCache {
    kind: RWType,
    peers: BTreeMap<(u64, u64), HotPeerStat>,
    denoising: bool,
}

impl HotPeerCache {
    /// Creates an empty cache for one direction, with short-report denoising on.
    pub fn new(kind: RWType) -> Self {
        assert!(matches!(kind, RWType::READ | RWType::WRITE));
        Self {
            kind,
            peers: BTreeMap::new(),
            denoising: true,
        }
    }

    /// Controls the Go simulator's minimum three-second report filter per cache.
    pub fn set_denoising(&mut self, enabled: bool) {
        self.denoising = enabled;
    }

    /// Borrows the current item. Snapshot callers can use HotPeerStat::clone.
    pub fn get(&self, region: u64, store: u64) -> Option<&HotPeerStat> {
        self.peers.get(&(region, store))
    }

    /// Applies Add/Update/Remove; unknown actions leave the cache unchanged.
    pub fn update_stat(&mut self, item: HotPeerStat) {
        let key = (item.region_id, item.store_id);
        match item.action_type {
            ActionType::ADD | ActionType::UPDATE => {
                self.peers.insert(key, item);
            }
            ActionType::REMOVE => {
                self.peers.remove(&key);
            }
            _ => {}
        }
    }

    /// Checks cumulative eight-kind deltas reported over whole seconds. None
    /// selects region peers; Some(empty) selects none. Explicit extra stores are
    /// retained, matching Go. The threshold provider returns byte/key/query/CPU.
    /// Returned actions must be applied before checking the next report. Checking
    /// is transactional: dropping an action does not advance the cached window.
    pub fn check_peer_flow(
        &self,
        region: &RegionInfo,
        peers: Option<&[u64]>,
        delta: &[f64],
        interval: u64,
        thresholds: impl Fn(u64) -> [f64; DIM_LEN],
    ) -> Vec<HotPeerStat> {
        if self.denoising && interval < 3 {
            return vec![];
        }
        let fallback: Vec<_>;
        let stores = match peers {
            Some(stores) => stores,
            None => {
                fallback = region.meta().peers.iter().map(|p| p.store_id).collect();
                &fallback
            }
        };
        let nanos = (interval as i64).wrapping_mul(SECOND);
        let mut result = Vec::with_capacity(stores.len());
        for &store in stores {
            let old = self.get(region.meta().id, store);
            let thresholds = thresholds(store);
            let loads = self.kind.get_load_rates(delta, interval);
            if old.is_none() && !loads.iter().zip(thresholds).any(|(v, t)| *v >= t) {
                continue;
            }
            let mut item = HotPeerStat {
                region_id: region.meta().id,
                store_id: store,
                loads: loads.to_vec(),
                stores: region.meta().peers.iter().map(|p| p.store_id).collect(),
                is_leader: region.leader().is_some_and(|p| p.store_id == store),
                action_type: ActionType::UPDATE,
                ..Default::default()
            };
            if let Some(old) = old {
                self.update_hot_peer_stat(&mut item, old, delta, nanos, &thresholds);
            } else {
                item.action_type = ActionType::ADD;
                if interval >= self.kind.report_interval() {
                    self.init_item(&mut item);
                }
                item.rolling_loads = Some(
                    self.kind
                        .region_stats()
                        .iter()
                        .map(|k| {
                            let mut dim = DimStat::new(self.report_nanos());
                            dim.add(delta[k.as_i32() as usize], nanos);
                            if dim.is_full(self.report_nanos()) {
                                dim.clear_last_average();
                            }
                            Some(dim)
                        })
                        .collect(),
                );
            }
            result.push(item);
        }
        result
    }

    fn report_nanos(&self) -> i64 {
        self.kind.report_interval() as i64 * SECOND
    }

    fn init_item(&self, item: &mut HotPeerStat) {
        item.hot_degree = 1;
        item.anti_count = self.kind.default_anti_count() as i64;
        item.allow_inherited = true;
    }

    fn update_hot_peer_stat(
        &self,
        item: &mut HotPeerStat,
        old: &HotPeerStat,
        delta: &[f64],
        nanos: i64,
        thresholds: &[f64],
    ) {
        item.hot_degree = old.hot_degree;
        item.anti_count = old.anti_count;
        item.allow_inherited = old.allow_inherited;
        item.last_transfer_leader_time = old.last_transfer_leader_time;
        item.rolling_loads = old.rolling_loads.as_ref().map(|loads| {
            loads
                .iter()
                .map(|d| d.as_ref().map(DimStat::copy_for_report))
                .collect()
        });
        let rolling = item
            .rolling_loads
            .as_mut()
            .expect("cached peers have rolling loads");
        for (dim, kind) in self.kind.region_stats().iter().enumerate() {
            rolling[dim]
                .as_mut()
                .expect("cached dimensions exist")
                .add(delta[kind.as_i32() as usize], nanos);
        }
        if !rolling[0].as_ref().unwrap().is_full(self.report_nanos()) {
            return;
        }
        if item.is_hot(thresholds) {
            item.hot_degree = old.hot_degree.wrapping_add(1);
            if old.anti_count < self.kind.default_anti_count() as i64 {
                item.anti_count = old.anti_count.wrapping_add(1);
            }
            item.allow_inherited = true;
        } else {
            item.hot_degree = old.hot_degree.wrapping_sub(1);
            item.anti_count = old.anti_count.wrapping_sub(1);
            if item.anti_count <= 0 {
                item.action_type = ActionType::REMOVE;
            } else {
                item.allow_inherited = true;
            }
        }
        item.clear_last_average();
    }

    /// Default minimum thresholds selected by this cache's direction.
    pub fn minimum_thresholds(&self) -> [f64; DIM_LEN] {
        std::array::from_fn(|dim| {
            MIN_HOT_THRESHOLDS[self.kind.region_stats()[dim].as_i32() as usize]
        })
    }
}
