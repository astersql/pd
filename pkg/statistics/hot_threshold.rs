// Copyright 2026 AsterSQL.

use std::{collections::BTreeMap, sync::Mutex};

use astersql_pd_core::RegionInfo;
use astersql_pd_statistics_utils::{DIM_LEN, TopN};

use crate::{HotPeerCache, HotPeerStat};

const TOP_N: usize = 60;
const HOT_THRESHOLD_RATIO: f64 = 0.8;

pub(crate) struct ThresholdSnapshot {
    updated: i64,
    rates: [f64; DIM_LEN],
}

pub(crate) struct HotThresholds {
    stores: BTreeMap<u64, TopN<HotPeerStat>>,
    pub(crate) snapshots: Mutex<BTreeMap<u64, ThresholdSnapshot>>,
    refresh_nanos: u64,
}

impl Default for HotThresholds {
    fn default() -> Self {
        Self {
            stores: BTreeMap::new(),
            snapshots: Mutex::new(BTreeMap::new()),
            refresh_nanos: 8_000_000_000,
        }
    }
}

impl HotThresholds {
    pub(crate) fn retain_stores(&mut self, active: &std::collections::BTreeSet<u64>) {
        self.stores.retain(|store, _| active.contains(store));
        self.snapshots
            .get_mut()
            .expect("threshold snapshot lock poisoned")
            .retain(|store, _| active.contains(store));
    }

    pub(crate) fn put(&mut self, item: &HotPeerStat) {
        // Freeze denoised loads; later pending reports must not mutate heap keys.
        self.stores
            .entry(item.store_id)
            .or_insert_with(|| TopN::new(DIM_LEN, TOP_N))
            .put(item.clone());
    }

    pub(crate) fn remove(&mut self, store: u64, region: u64) {
        if let Some(top) = self.stores.get_mut(&store) {
            top.remove(region);
        }
    }
}

impl HotPeerCache {
    /// Returns per-store byte/key/query/CPU thresholds. The host supplies signed
    /// nanoseconds on a consistent clock. A cached value stays valid through the
    /// refresh boundary (and clock rollback), matching Go's inclusive time check.
    /// Each dimension uses max(60th largest denoised load * 0.8, minimum).
    /// Like other cache operations, callers must serialize access.
    pub fn calc_hot_thresholds(&self, store: u64, now: i64) -> [f64; DIM_LEN] {
        let state = &self.hot_thresholds;
        let mut snapshots = state
            .snapshots
            .lock()
            .expect("threshold snapshot lock poisoned");
        if let Some(snapshot) = snapshots.get(&store)
            && now as i128 - snapshot.updated as i128 <= state.refresh_nanos as i128
        {
            return snapshot.rates;
        }
        let mut rates = self.minimum_thresholds();
        if let Some(top) = state.stores.get(&store)
            && top.len() >= TOP_N
        {
            for (dim, rate) in rates.iter_mut().enumerate() {
                let load = top
                    .get_top_n_min(dim)
                    .expect("nonempty TopN")
                    .get_load(dim as isize);
                *rate = (load * HOT_THRESHOLD_RATIO).max(*rate);
            }
        }
        snapshots.insert(
            store,
            ThresholdSnapshot {
                updated: now,
                rates,
            },
        );
        rates
    }

    /// Changes the cache-local refresh interval in nanoseconds (default 8s).
    /// Zero recomputes on the next strictly later clock sample; changing this
    /// interval does not otherwise invalidate existing snapshots.
    pub fn set_thresholds_update_interval(&mut self, nanos: u64) {
        self.hot_thresholds.refresh_nanos = nanos;
    }

    /// Checks a report using this cache's real per-store TopN thresholds.
    /// Apply returned actions with update_stat before checking the next report.
    /// A discarded action leaves peer windows and rank membership unchanged.
    pub fn check_peer_flow_dynamic(
        &self,
        region: &RegionInfo,
        peers: Option<&[u64]>,
        delta: &[f64],
        interval: u64,
        now: i64,
    ) -> Vec<HotPeerStat> {
        self.check_peer_flow_at(region, peers, delta, interval, now, |store| {
            self.calc_hot_thresholds(store, now)
        })
    }
}
