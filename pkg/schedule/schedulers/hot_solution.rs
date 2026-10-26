// Copyright 2026 AsterSQL.
//! Hot solution load views and base scheduler pending accounting.
use crate::{HotRegionParams, PendingInfluence};
use astersql_pd_schedule_operator::Clock;
use astersql_pd_statistics::{
    HotPeerStat, Loads, StoreHistoryLoads, StoreLoadDetail, StoreSummaryInfo,
};
use std::{collections::HashMap, time::Duration};

/// A borrowed candidate's load projection; operator construction and ranking belong to the solver.
/// Recompute rates whenever peers or the selected dimensions change.
pub struct HotSolution<'a> {
    pub src_store: &'a StoreLoadDetail,
    pub dst_store: &'a StoreLoadDetail,
    pub main_peer: &'a HotPeerStat,
    pub revert_peer: Option<&'a HotPeerStat>,
    cached_peers_rate: Option<Loads>,
}
impl<'a> HotSolution<'a> {
    pub fn new(
        src_store: &'a StoreLoadDetail,
        dst_store: &'a StoreLoadDetail,
        main_peer: &'a HotPeerStat,
        revert_peer: Option<&'a HotPeerStat>,
    ) -> Self {
        Self {
            src_store,
            dst_store,
            main_peer,
            revert_peer,
            cached_peers_rate: None,
        }
    }
    /// Reset all dimensions, then cache main minus revert using denoised peer loads.
    /// Invalid dimensions panic, as Go's cache indexing does.
    pub fn calc_peers_rate(&mut self, dims: &[usize]) {
        let mut rates = Loads::default();
        for &dim in dims {
            rates[dim] = self.main_peer.get_load(dim as isize)
                - self
                    .revert_peer
                    .map_or(0., |peer| peer.get_load(dim as isize));
        }
        self.cached_peers_rate = Some(rates);
    }
    /// Panics before calculation or for an invalid dimension, matching the Go precondition.
    pub fn peers_rate(&self, dim: usize) -> f64 {
        self.cached_peers_rate
            .as_ref()
            .expect("calculate peer rates before reading the cache")[dim]
    }
    pub fn current_load(&self, dim: usize) -> (f64, f64) {
        (
            self.src_store.load_pred.current.loads[dim],
            self.dst_store.load_pred.current.loads[dim],
        )
    }
    /// Pending rates are absolute differences between current and future loads.
    pub fn pending_load(&self, dim: usize) -> (f64, f64) {
        (
            self.src_store.load_pred.pending().loads[dim],
            self.dst_store.load_pred.pending().loads[dim],
        )
    }
    /// Closest predicted source/destination loads; reverse extremes for a negative net peer rate.
    pub fn extreme_load(&self, dim: usize) -> (f64, f64) {
        if self.peers_rate(dim) >= 0. {
            (
                self.src_store.load_pred.min().loads[dim],
                self.dst_store.load_pred.max().loads[dim],
            )
        } else {
            (
                self.src_store.load_pred.max().loads[dim],
                self.dst_store.load_pred.min().loads[dim],
            )
        }
    }
}

/// Compatibility fallback for old or non-hot scheduler configuration.
/// Configuration validation remains owned by HotRegionConfig.
pub fn effective_hot_pending_weight(config: Option<&HotRegionParams>) -> f64 {
    match config {
        Some(p) if p.pending_weight < 1. => 1.,
        Some(p) => p.pending_weight,
        None => 1.,
    }
}

/// Base hot scheduler state, without the later Schedule/solver integration.
/// The caller exclusively owns this state and supplies a fresh summary snapshot each round.
#[derive(Default)]
pub struct HotLoadProjection {
    pub region_pendings: HashMap<u64, PendingInfluence>,
    pub history: StoreHistoryLoads,
}
impl HotLoadProjection {
    /// Preserve existing history when unchanged; reset it when either sampling value changes.
    pub fn update_history_load_config(&mut self, duration: Duration, interval: Duration) -> bool {
        self.history.update_config(duration, interval)
    }
    /// Refresh real operator status and sum signed influence per source occurrence.
    /// A missing source still contributes to an existing destination. Failed terminal
    /// operators have no influence but remain retained until their zombie window ends.
    /// Empty source lists are left untouched, matching Go's inner-loop GC semantics.
    /// Metrics publication belongs to the host with access to fresh store liveness.
    pub fn summary_pending_influence(
        &mut self,
        stores: &mut HashMap<u64, StoreSummaryInfo>,
        config: Option<&HotRegionParams>,
        clock: &dyn Clock,
    ) {
        let pending_weight = effective_hot_pending_weight(config);
        self.region_pendings.retain(|_, pending| {
            let mut keep = true;
            for from in &pending.froms {
                let (weight, gc) = pending.weight(clock);
                if gc {
                    keep = false;
                    continue;
                }
                let weight = weight * pending_weight;
                if weight > 0. {
                    if let Some(source) = stores.get_mut(from) {
                        source.add_influence(Some(&pending.origin), -weight);
                    }
                    if let Some(destination) = stores.get_mut(&pending.to) {
                        destination.add_influence(Some(&pending.origin), weight);
                    }
                }
            }
            keep
        });
    }
}
