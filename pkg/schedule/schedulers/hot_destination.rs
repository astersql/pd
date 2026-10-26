// Copyright 2026 AsterSQL.
//! Destination admission using canonical load predictions and placement safeguards.
use crate::{HotRegionParams, HotSourceOperation};
use astersql_pd_core::{RegionInfo, StoreInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    AsyncFilter, ExcludedFilter, Filter, HotRegionEvictedTargetFilter, PlacementGuardContext,
    SPECIAL_USE_HOT_REGION, SpecialUseFilter, StoreStateFilter, SynchronousFilter,
};
use astersql_pd_schedule_placement::RegionFit;
use astersql_pd_statistics::{HotPeerStat, StoreLoad, StoreLoadDetail};
use astersql_pd_statistics_utils::RWType;
use std::sync::Arc;

/// A resolved placement population, borrowed from the host's current solver round.
pub struct HotDestinationScope<'a> {
    pub store_ids: &'a [u64],
    pub expect: &'a StoreLoad,
}
/// Invalid destination priority configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotDestinationError {
    #[error("hot destination requires two distinct dimensions in 0..4")]
    InvalidPriorities,
}
/// Load diagnostic; store filters retain their own canonical status semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotDestinationDecision {
    Eligible,
    OutOfScope,
    LoadFailed,
    HistoryFailed,
}
impl HotDestinationDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Eligible => "dst-store-succ",
            Self::OutOfScope => "dst-store-out-of-scope",
            Self::LoadFailed => "dst-store-failed",
            Self::HistoryFailed => "dst-store-history-loads-failed",
        }
    }
    pub fn admitted(self) -> bool {
        self == Self::Eligible
    }
}
/// One immutable configuration snapshot. Scheduling runs on the caller's runtime.
pub struct HotDestinationSelector {
    config: HotRegionParams,
    priorities: [usize; 2],
    rw: RWType,
    operation: HotSourceOperation,
}
impl HotDestinationSelector {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        rw: RWType,
        operation: HotSourceOperation,
    ) -> Result<Self, HotDestinationError> {
        if priorities.iter().any(|&d| d >= 4) || priorities[0] == priorities[1] {
            return Err(HotDestinationError::InvalidPriorities);
        }
        Ok(Self {
            config,
            priorities,
            rw,
            operation,
        })
    }
    fn by_priority(&self, mut predicate: impl FnMut(usize) -> bool) -> bool {
        if self.rw == RWType::WRITE && self.operation == HotSourceOperation::TransferLeader
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
    fn engine_allowed(&self, store: &StoreInfo) -> bool {
        store.is_tikv()
            || self.config.enable_for_tiflash
                && !store.is_tiflash_compute()
                && self.rw == RWType::WRITE
                && self.operation == HotSourceOperation::MovePeer
    }
    /// Scoped membership is checked before thresholds. Current/future maxima and
    /// every current history sample must pass strict tolerance comparisons.
    /// Misaligned histories are a programmer precondition, matching Go's panic.
    pub fn destination_store_decision(
        &self,
        detail: &StoreLoadDetail,
        scope: Option<&HotDestinationScope<'_>>,
    ) -> HotDestinationDecision {
        let store = &detail.store_summary_info.store_info;
        if scope.is_some_and(|s| !s.store_ids.contains(&store.id())) {
            return HotDestinationDecision::OutOfScope;
        }
        let expect = scope.map_or(&detail.load_pred.expect, |s| s.expect);
        let ratio = self.config.dst_tolerance_ratio + if store.is_tikv() { 0. } else { 0.1 };
        let max = detail.load_pred.max();
        if !self.by_priority(|d| max.loads[d] * ratio < expect.loads[d]) {
            return HotDestinationDecision::LoadFailed;
        }
        if !self.by_priority(|d| {
            detail.load_pred.current.history_loads[d]
                .iter()
                .enumerate()
                .all(|(j, &v)| v * ratio < expect.history_loads[d][j])
        }) {
            return HotDestinationDecision::HistoryFailed;
        }
        HotDestinationDecision::Eligible
    }
    /// Apply engine support, ordered target filters, then destination thresholds.
    /// None means legacy/global expectation; a scope enables placement-v2 checks.
    /// Returned order is not a rank (the Go result is a map).
    pub async fn pick_dst_stores<'a, C: SharedConfigProvider>(
        &self,
        candidates: &[&'a StoreLoadDetail],
        filters: &[&dyn AsyncFilter<C::Version>],
        conf: &C,
        scope: Option<&HotDestinationScope<'_>>,
    ) -> Vec<&'a StoreLoadDetail> {
        let mut ret = Vec::with_capacity(candidates.len());
        for &detail in candidates {
            let store = &detail.store_summary_info.store_info;
            if !self.engine_allowed(store) {
                continue;
            }
            let mut allowed = true;
            for filter in filters {
                if !filter.target_async(conf, store).await.is_ok() {
                    allowed = false;
                    break;
                }
            }
            if allowed && self.destination_store_decision(detail, scope).admitted() {
                ret.push(detail);
            }
        }
        ret
    }
    /// Assemble the actual move-peer/read-move-leader/write-transfer-leader
    /// candidate paths. Context, source and peer must refer to the current round;
    /// the caller obtains a healthy region/fit through HotSourceSelector first.
    pub async fn filter_dst_stores<'a, C: SharedConfigProvider>(
        &self,
        details: &'a [StoreLoadDetail],
        source: &StoreInfo,
        main_peer: &HotPeerStat,
        context: &mut PlacementGuardContext<'_, C>,
        fit: Option<Arc<RegionFit>>,
        scope: Option<&HotDestinationScope<'_>>,
    ) -> Vec<&'a StoreLoadDetail> {
        let moving = self.operation == HotSourceOperation::MovePeer;
        if moving && self.rw == RWType::READ && main_peer.is_leader
            || !moving && !main_peer.is_leader
        {
            return Vec::new();
        }
        let state = StoreStateFilter {
            action_scope: "hot-region".into(),
            move_region: moving,
            transfer_leader: !moving,
            operator_level: PriorityLevel::HIGH,
            now_unix_nanos: context.now,
            ..Default::default()
        };
        let move_leader_state = StoreStateFilter {
            move_region: true,
            operator_level: PriorityLevel::HIGH,
            now_unix_nanos: context.now,
            ..Default::default()
        };
        let evicted = HotRegionEvictedTargetFilter::new("hot-region");
        let special = SpecialUseFilter::new("hot-region", &[SPECIAL_USE_HOT_REGION.into()]);
        let excluded = ExcludedFilter::new("hot-region", [], context.region.store_ids());
        let safeguard: Option<Box<dyn Filter<C::Version>>> = if moving {
            Some(context.safeguard("hot-region", source, fit))
        } else {
            context
                .leader_safeguard("hot-region", source.id(), fit, self.rw == RWType::READ)
                .map(|f| Box::new(f) as Box<dyn Filter<C::Version>>)
        };
        let mut sync: Vec<&dyn Filter<C::Version>> = Vec::new();
        if moving {
            sync.extend([&evicted as &dyn Filter<C::Version>, &excluded]);
        }
        sync.push(&special);
        if let Some(guard) = &safeguard {
            sync.push(guard.as_ref());
        }
        let adapted: Vec<_> = sync.iter().map(|&f| SynchronousFilter(f)).collect();
        let mut filters: Vec<&dyn AsyncFilter<C::Version>> = vec![&state];
        filters.extend(adapted.iter().map(|f| f as &dyn AsyncFilter<C::Version>));
        let mut candidates = Vec::with_capacity(details.len());
        let followers = context.region.followers();
        for detail in details {
            let store = &detail.store_summary_info.store_info;
            if !moving {
                if self.rw == RWType::WRITE {
                    if !followers.contains_key(&store.id()) {
                        continue;
                    }
                } else {
                    if store.id() == main_peer.store_id {
                        continue;
                    }
                    if context.region.get_store_peer(store.id()).is_none()
                        && (!move_leader_state.target(context.conf, store).await.is_ok()
                            || !evicted.target(context.conf, store).is_ok())
                    {
                        continue;
                    }
                }
            }
            candidates.push(detail);
        }
        self.pick_dst_stores(&candidates, &filters, context.conf, scope)
            .await
    }
    /// Reverse transfer requires an existing peer for leader actions and an absent
    /// peer for peer moves. Same-region and health checks belong to the solver.
    pub fn allow_revert_region(&self, region: &RegionInfo, original_source: u64) -> bool {
        region.get_store_peer(original_source).is_some()
            == (self.operation == HotSourceOperation::TransferLeader)
    }
    /// Protect a reverse region using reversed source/target and its own fit.
    pub fn is_revert_region_valid<C: SharedConfigProvider>(
        &self,
        context: &mut PlacementGuardContext<'_, C>,
        reverse_source: &StoreInfo,
        reverse_target: &StoreInfo,
        fit: Option<Arc<RegionFit>>,
    ) -> bool {
        match self.operation {
            HotSourceOperation::MovePeer => context
                .safeguard("hot-region", reverse_source, fit)
                .target(context.conf, reverse_target)
                .is_ok(),
            HotSourceOperation::TransferLeader => context
                .leader_safeguard(
                    "hot-region",
                    reverse_source.id(),
                    fit,
                    self.rw == RWType::READ,
                )
                .is_none_or(|guard| guard.target(context.conf, reverse_target).is_ok()),
        }
    }
}
