// Copyright 2026 AsterSQL.
//! Source admission over canonical store statistics, hot peers and region placement.
use crate::HotRegionParams;
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    RegionReplicationContext, is_region_healthy_allow_pending, is_region_replicated,
};
use astersql_pd_schedule_placement::RegionFit;
use astersql_pd_statistics::{HotPeerStat, StoreLoad, StoreLoadDetail};
use astersql_pd_statistics_utils::RWType;
use std::{collections::HashSet, sync::Arc};

/// Candidate action determines engine support and stale-stat membership checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotSourceOperation {
    MovePeer,
    TransferLeader,
}
/// Invalid construction or a region rejected by the source solver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotSourceError {
    #[error("hot source requires two distinct dimensions in 0..4")]
    InvalidPriorities,
    #[error("hot source peer limit must be nonnegative")]
    InvalidPeerLimit,
    #[error("region is absent")]
    MissingRegion,
    #[error("region has unhealthy replicas")]
    UnhealthyRegion,
    #[error("region has abnormal replica count or unsatisfied placement rules")]
    AbnormalReplicas,
    #[error("source peer statistics are stale")]
    StalePeer,
    #[error("source leader statistics are stale")]
    StaleLeader,
}
/// Admission result; the host publishes corresponding source-store counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotSourceDecision {
    Eligible,
    Deferred,
    Ineligible,
    LoadFailed,
    HistoryFailed,
}
impl HotSourceDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eligible => "src-store-succ",
            Self::Deferred => "deferred",
            Self::Ineligible => "ineligible",
            Self::LoadFailed => "src-store-failed",
            Self::HistoryFailed => "src-store-history-loads-failed",
        }
    }
    pub fn admitted(self) -> bool {
        matches!(self, Self::Eligible | Self::Deferred)
    }
}
/// Borrowed peer set plus per-priority nth peers, calculated before exclusions.
/// Union order is unspecified, like Go map iteration; callers must not rank by it.
pub struct HotSourcePeers<'a> {
    pub peers: Vec<&'a HotPeerStat>,
    pub nth: [Option<&'a HotPeerStat>; 2],
}
/// Configuration snapshot for one source selection round. No private runtime or I/O.
pub struct HotSourceSelector {
    config: HotRegionParams,
    priorities: [usize; 2],
    rw: RWType,
    operation: HotSourceOperation,
}
impl HotSourceSelector {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        rw: RWType,
        operation: HotSourceOperation,
    ) -> Result<Self, HotSourceError> {
        if priorities.iter().any(|&d| d >= 4) || priorities[0] == priorities[1] {
            return Err(HotSourceError::InvalidPriorities);
        }
        if config.max_peer_number < 0 {
            return Err(HotSourceError::InvalidPeerLimit);
        }
        Ok(Self {
            config,
            priorities,
            rw,
            operation,
        })
    }
    fn write_leader(&self) -> bool {
        self.rw == RWType::WRITE && self.operation == HotSourceOperation::TransferLeader
    }
    fn by_priority(&self, mut predicate: impl FnMut(usize) -> bool) -> bool {
        if self.write_leader()
            || self.config.rank_formula_version == "v1" && !self.config.strict_picking_store
        {
            predicate(self.priorities[0])
        } else if self.config.rank_formula_version == "v1" {
            (0..4)
                .filter(|d| self.priorities.contains(d))
                .all(predicate)
        } else {
            (0..4)
                .filter(|d| self.priorities.contains(d))
                .any(predicate)
        }
    }
    /// A scoped population's expectation overrides the global engine expectation.
    pub fn expect_load<'a>(
        &self,
        detail: &'a StoreLoadDetail,
        scope: Option<&'a StoreLoad>,
    ) -> &'a StoreLoad {
        scope.unwrap_or(&detail.load_pred.expect)
    }
    /// The caller owns scope population discovery; this evaluates its actual expectation.
    /// Mismatched history lengths panic at the accessed sample, matching Go's contract.
    pub fn source_store_decision(
        &self,
        detail: &StoreLoadDetail,
        scope: Option<&StoreLoad>,
    ) -> HotSourceDecision {
        let store = &detail.store_summary_info.store_info;
        let ratio = self.config.src_tolerance_ratio + if store.is_tikv() { 0. } else { 0.1 };
        let expect = self.expect_load(detail, scope);
        let min = detail.load_pred.min();
        if !self.by_priority(|d| min.loads[d] > ratio * expect.loads[d]) {
            return HotSourceDecision::LoadFailed;
        }
        if !self.by_priority(|d| {
            detail.load_pred.current.history_loads[d]
                .iter()
                .enumerate()
                .all(|(j, &v)| v > ratio * expect.history_loads[d][j])
        }) {
            return HotSourceDecision::HistoryFailed;
        }
        HotSourceDecision::Eligible
    }
    /// Return each store's admission and diagnostic. Deferred stores must be checked
    /// with source_store_decision after the host resolves their placement scope.
    /// can_restrict uses [TiKV, TiFlash], as the Go placement population index does.
    pub fn filter_src_stores<'a>(
        &self,
        stores: &'a [StoreLoadDetail],
        placement_v2: bool,
        can_restrict: [bool; 2],
    ) -> Vec<(&'a StoreLoadDetail, HotSourceDecision)> {
        stores
            .iter()
            .map(|detail| {
                let store = &detail.store_summary_info.store_info;
                let tikv = store.is_tikv();
                let decision = if (!tikv
                    && (!self.config.enable_for_tiflash
                        || store.is_tiflash_compute()
                        || self.rw != RWType::WRITE
                        || self.operation != HotSourceOperation::MovePeer))
                    || detail.hot_peers.is_empty()
                {
                    HotSourceDecision::Ineligible
                } else if placement_v2 && can_restrict[usize::from(!tikv)] {
                    HotSourceDecision::Deferred
                } else {
                    self.source_store_decision(detail, None)
                };
                (detail, decision)
            })
            .collect()
    }
    /// Alternate unique pointers from the two descending priority lists. Peers with
    /// equal rates have no stable Go tie order; pointer identity, not region ID, deduplicates.
    pub fn sort_hot_peers<'a>(
        first: &[&'a HotPeerStat],
        second: &[&'a HotPeerStat],
        max: usize,
    ) -> Vec<&'a HotPeerStat> {
        let mut selected = Vec::with_capacity(max.min(first.len() + second.len()));
        let mut seen = HashSet::new();
        let mut a = first.iter();
        let mut b = second.iter();
        while selected.len() < max {
            let mut added = false;
            for list in [&mut a, &mut b] {
                if selected.len() == max {
                    break;
                }
                for &peer in list {
                    if seen.insert(peer as *const HotPeerStat) {
                        selected.push(peer);
                        added = true;
                        break;
                    }
                }
            }
            if !added {
                break;
            }
        }
        selected
    }
    /// top_n_position is normally 10; zero disables nth extraction. Explicit now
    /// is host nanoseconds and cooldown uses the real HotPeerStat implementation.
    pub fn filter_hot_peers<'a>(
        &self,
        peers: &'a [HotPeerStat],
        pending: &HashSet<u64>,
        now: i64,
        min_hot_degree: i64,
        top_n_position: usize,
    ) -> HotSourcePeers<'a> {
        let mut first = Vec::new();
        let mut second = Vec::new();
        let mut nth = [None; 2];
        if (top_n_position > 0 && peers.len() >= top_n_position)
            || peers.len() > self.config.max_peer_number as usize
        {
            first = peers.iter().collect();
            second = first.clone();
            let sort = |list: &mut Vec<&HotPeerStat>, d: usize| {
                list.sort_unstable_by(|a, b| {
                    b.get_load(d as isize)
                        .partial_cmp(&a.get_load(d as isize))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            };
            sort(&mut first, self.priorities[0]);
            sort(&mut second, self.priorities[1]);
            if top_n_position > 0 && peers.len() >= top_n_position {
                nth = [
                    Some(first[top_n_position - 1]),
                    Some(second[top_n_position - 1]),
                ];
            }
        }
        let candidates = if peers.len() > self.config.max_peer_number as usize {
            Self::sort_hot_peers(&first, &second, self.config.max_peer_number as usize)
        } else {
            peers.iter().collect()
        };
        HotSourcePeers {
            peers: candidates
                .into_iter()
                .filter(|p| {
                    !pending.contains(&p.region_id)
                        && !p.needs_cooldown_transfer_leader(now, min_hot_degree, self.rw)
                })
                .collect(),
            nth,
        }
    }
    /// Pending replicas are allowed. Return the real placement fit for later
    /// safeguards; never cache a successful fit across rejected regions.
    pub fn get_region<'a, C: SharedConfigProvider>(
        &self,
        region: Option<&'a RegionInfo>,
        source_store: u64,
        context: &RegionReplicationContext<'_, C>,
    ) -> Result<(&'a RegionInfo, Option<Arc<RegionFit>>), HotSourceError> {
        let region = region.ok_or(HotSourceError::MissingRegion)?;
        if !is_region_healthy_allow_pending(region) {
            return Err(HotSourceError::UnhealthyRegion);
        }
        let fit = if context.conf.is_placement_rules_enabled() {
            let fit = context.manager.borrow_mut().fit_region(
                context.stores,
                region,
                context.conf,
                context.now,
            );
            if !fit.is_satisfied() {
                return Err(HotSourceError::AbnormalReplicas);
            }
            Some(fit)
        } else {
            if !is_region_replicated(context, region) {
                return Err(HotSourceError::AbnormalReplicas);
            }
            None
        };
        match self.operation {
            HotSourceOperation::MovePeer if region.get_store_peer(source_store).is_none() => {
                return Err(HotSourceError::StalePeer);
            }
            HotSourceOperation::TransferLeader
                if region.leader().map_or(0, |p| p.store_id) != source_store =>
            {
                return Err(HotSourceError::StaleLeader);
            }
            _ => {}
        }
        Ok((region, fit))
    }
    /// Statistics provenance selects the pending influence's zombie window.
    pub fn max_zombie_duration(&self, source: &StoreLoadDetail) -> astersql_pd_typeutil::Duration {
        if self.write_leader() && self.priorities[0] != 2
            || self.rw == RWType::WRITE
                && self.operation == HotSourceOperation::MovePeer
                && source.store_summary_info.store_info.is_tiflash_write()
        {
            self.config.region_stat_zombie_duration()
        } else {
            self.config.store_stat_zombie_duration()
        }
    }
}
