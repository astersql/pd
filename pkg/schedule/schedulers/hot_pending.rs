// Copyright 2026 AsterSQL.
//! Hot pending admission; operator status and GC remain in HotLoadProjection.
use crate::{
    HotDispatchResult, HotLoadProjection, HotRegionParams, HotResource, HotSolveResult,
    HotSourceSelector, PendingInfluence,
};
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_operator::{OpKind, Operator};
use astersql_pd_statistics::{HotPeerStat, Influence};
use astersql_pd_statistics_utils::{REGION_STAT_COUNT, RWType};
use std::{collections::hash_map::Entry, sync::Arc, time::Duration};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotPendingError {
    #[error(transparent)]
    Source(#[from] crate::HotSourceError),
    #[error("region {0} already has pending influence")]
    AlreadyPending(u64),
    #[error("hot pending offer has no executable solution")]
    MissingSolution,
    #[error("hot pending source and destination engines differ")]
    EngineMismatch,
    #[error("hot pending operator and region do not match")]
    StaleRegion,
    #[error("inverse hot peer identity does not match")]
    StaleInverse,
    #[error("hot peer loads must contain four dimensions")]
    ShortLoads,
}
/// Preserve all read/write dimensions, including CPU, using the real cache's
/// same-region/same-store inverse peer supplied by the host snapshot.
pub fn collect_hot_pending_influence(
    rw: RWType,
    peer: &HotPeerStat,
    inverse: Option<&HotPeerStat>,
) -> Result<Influence, HotPendingError> {
    if inverse.is_some_and(|p| p.region_id != peer.region_id || p.store_id != peer.store_id) {
        return Err(HotPendingError::StaleInverse);
    }
    let loads = peer.get_loads();
    let other = inverse.map(HotPeerStat::get_loads);
    if loads.len() < 4 || other.as_ref().is_some_and(|v| v.len() < 4) {
        return Err(HotPendingError::ShortLoads);
    }
    let mut infl = Influence {
        loads: vec![0.; REGION_STAT_COUNT],
        hot_peer_count: 1.,
    };
    rw.set_full_load_rates(&mut infl.loads, &loads);
    if let Some(other) = other {
        rw.inverse().set_full_load_rates(&mut infl.loads, &other);
    }
    Ok(infl)
}
impl HotLoadProjection {
    /// Exclusive mutable ownership makes single-region check/insert indivisible.
    /// A duplicate never changes the retained operator or its original influence.
    pub fn try_add_pending_influence(
        &mut self,
        op: Arc<Operator>,
        froms: Vec<u64>,
        to: u64,
        origin: Influence,
        max_zombie_duration: Duration,
    ) -> Result<(), HotPendingError> {
        let id = op.region_id();
        match self.region_pendings.entry(id) {
            Entry::Occupied(_) => Err(HotPendingError::AlreadyPending(id)),
            Entry::Vacant(entry) => {
                entry.insert(PendingInfluence {
                    op,
                    froms,
                    to,
                    origin,
                    max_zombie_duration,
                });
                Ok(())
            }
        }
    }
    /// Split subtracts the peer's load from every replica, with no destination.
    pub fn admit_hot_split(
        &mut self,
        op: Arc<Operator>,
        region: &RegionInfo,
        rw: RWType,
        peer: &HotPeerStat,
        inverse: Option<&HotPeerStat>,
        max_zombie_duration: Duration,
    ) -> Result<(), HotPendingError> {
        if op.kind() != OpKind::SPLIT
            || op.region_id() != region.meta().id
            || peer.region_id != region.meta().id
        {
            return Err(HotPendingError::StaleRegion);
        }
        let infl = collect_hot_pending_influence(rw, peer, inverse)?;
        self.try_add_pending_influence(
            op,
            region.store_ids().into_iter().collect(),
            0,
            infl,
            max_zombie_duration,
        )
    }
    /// Match Go's sequential main then reverse insertion. If reverse admission
    /// conflicts, the main record remains pending (the Go atomicity TODO).
    pub fn admit_hot_solution(
        &mut self,
        resource: HotResource,
        result: &HotSolveResult<'_>,
        config: &HotRegionParams,
        mut inverse: impl FnMut(RWType, u64, u64) -> Option<HotPeerStat>,
    ) -> Result<(), HotPendingError> {
        let best = result
            .best
            .as_ref()
            .ok_or(HotPendingError::MissingSolution)?;
        let main = result
            .operators
            .first()
            .ok_or(HotPendingError::MissingSolution)?;
        let src = &best.source.store_summary_info.store_info;
        let dst = &best.destination.store_summary_info.store_info;
        if src.is_tiflash_write() != dst.is_tiflash_write()
            || src.is_tiflash_compute() != dst.is_tiflash_compute()
        {
            return Err(HotPendingError::EngineMismatch);
        }
        if main.region_id() != best.region.meta().id || main.region_id() != best.main_peer.region_id
        {
            return Err(HotPendingError::StaleRegion);
        }
        let duration = HotSourceSelector::new(
            config.clone(),
            [
                if resource == HotResource::WriteLeader
                    && config
                        .write_leader_priorities
                        .first()
                        .is_some_and(|s| s == "query")
                {
                    2
                } else {
                    0
                },
                1,
            ],
            resource.rw(),
            resource.operation(),
        )?
        .max_zombie_duration(best.source)
        .as_nanos();
        // Signed Go durations <= 0 expire immediately at the first terminal summary.
        let duration = Duration::from_nanos(duration.max(0) as u64);
        let other = inverse(
            resource.rw().inverse(),
            best.main_peer.region_id,
            best.main_peer.store_id,
        );
        let infl = collect_hot_pending_influence(resource.rw(), best.main_peer, other.as_ref())?;
        self.try_add_pending_influence(main.clone(), vec![src.id()], dst.id(), infl, duration)?;
        if let (Some(peer), Some(op)) = (best.revert_peer, result.operators.get(1)) {
            if op.region_id() != peer.region_id {
                return Err(HotPendingError::StaleRegion);
            }
            let other = inverse(resource.rw().inverse(), peer.region_id, peer.store_id);
            let infl = collect_hot_pending_influence(resource.rw(), peer, other.as_ref())?;
            self.try_add_pending_influence(op.clone(), vec![dst.id()], src.id(), infl, duration)?;
        }
        Ok(())
    }
}
impl<'a> HotDispatchResult<'a> {
    /// Try ranked read offers in order. Return the first admitted real solution;
    /// expose typed rejections to the host without submitting operators here.
    pub fn admit_pending(
        self,
        pending: &mut HotLoadProjection,
        config: &HotRegionParams,
        mut inverse: impl FnMut(RWType, u64, u64) -> Option<HotPeerStat>,
    ) -> (
        Option<(HotResource, HotSolveResult<'a>)>,
        Vec<HotPendingError>,
    ) {
        let mut errors = vec![];
        for (resource, result) in self.offers {
            match pending.admit_hot_solution(resource, &result, config, &mut inverse) {
                Ok(()) => return (Some((resource, result)), errors),
                Err(error) => errors.push(error),
            }
        }
        (None, errors)
    }
}
