// Copyright 2026 AsterSQL.
//! Read/write dispatch over admitted immutable candidates and canonical solvers.
use crate::{
    HotConfigError, HotRegionParams, HotSolveCandidate, HotSolveContext, HotSolveError,
    HotSolveResult, HotSolver, HotSourceOperation, IntervalGrowth, interval_grow,
};
use astersql_pd_statistics::StoreLoad;
use astersql_pd_statistics_utils::RWType;

/// The four resource indexes preserve Go's resourceType order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum HotResource {
    WritePeer,
    WriteLeader,
    ReadPeer,
    ReadLeader,
}
impl HotResource {
    pub const ALL: [Self; 4] = [
        Self::WritePeer,
        Self::WriteLeader,
        Self::ReadPeer,
        Self::ReadLeader,
    ];
    pub fn rw(self) -> RWType {
        match self {
            Self::WritePeer | Self::WriteLeader => RWType::WRITE,
            _ => RWType::READ,
        }
    }
    pub fn operation(self) -> HotSourceOperation {
        match self {
            Self::WriteLeader | Self::ReadLeader => HotSourceOperation::TransferLeader,
            _ => HotSourceOperation::MovePeer,
        }
    }
}
/// Per-resource population extrema and rank steps, prepared from the host snapshot.
#[derive(Clone, Default)]
pub struct HotRankInputs {
    pub max_source: StoreLoad,
    pub min_destination: StoreLoad,
    pub step: StoreLoad,
}
#[derive(Debug, thiserror::Error)]
pub enum HotScheduleError {
    #[error(transparent)]
    Config(#[from] HotConfigError),
    #[error(transparent)]
    Solve(#[from] HotSolveError),
    #[error("hot scheduler has no resources to sample")]
    EmptyResources,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotScheduleDiagnostic {
    Forbidden,
    NoSolution,
}
/// Ordered executable offers. The pending owner (task 95) admits the first offer
/// by region or tries the fallback; dispatch does not submit or register influence.
pub struct HotDispatchResult<'a> {
    pub offers: Vec<(HotResource, HotSolveResult<'a>)>,
    pub diagnostic: Option<HotScheduleDiagnostic>,
    pub errors: Vec<astersql_pd_schedule_operator::BuilderError>,
}
/// Four independent solver search states. RNG, preparation, cache admission and
/// pending lifecycle belong to the host; Schedule receives the sampled random word.
pub struct HotScheduler {
    config: HotRegionParams,
    solvers: [HotSolver; 4],
}
impl HotScheduler {
    pub fn new(
        config: HotRegionParams,
        query_supported: bool,
        cpu_supported: bool,
        inputs: [HotRankInputs; 4],
    ) -> Result<Self, HotScheduleError> {
        config.validate()?;
        let config = config.effective(query_supported, cpu_supported);
        let mut solvers = Vec::new();
        for (resource, input) in HotResource::ALL.into_iter().zip(inputs) {
            let names = match resource {
                HotResource::WritePeer => &config.write_peer_priorities,
                HotResource::WriteLeader => &config.write_leader_priorities,
                _ => &config.read_priorities,
            };
            let dims = std::array::from_fn(|i| match names[i].as_str() {
                "byte" => 0,
                "key" => 1,
                "query" => 2,
                "cpu" => 3,
                _ => unreachable!("effective priorities"),
            });
            solvers.push(HotSolver::new(
                config.clone(),
                dims,
                resource.rw(),
                resource.operation(),
                input.max_source,
                input.min_destination,
                input.step,
            )?);
        }
        Ok(Self {
            config,
            solvers: solvers.try_into().ok().expect("four resources"),
        })
    }
    /// Refresh population ranks without replacing the restored policy or reverse-search state.
    pub fn refresh_rank_inputs(&mut self, inputs: [HotRankInputs; 4]) -> Result<(), HotSolveError> {
        for (solver, input) in self.solvers.iter_mut().zip(inputs) {
            solver.refresh_rank_inputs(input)?;
        }
        Ok(())
    }
    pub fn min_interval() -> i64 {
        1_000_000_000
    }
    /// Go always grows the minimum interval, ignoring the previous duration.
    pub fn next_interval(_previous: i64) -> i64 {
        interval_grow(
            Self::min_interval(),
            20_000_000_000,
            IntervalGrowth::Exponential,
        )
    }
    pub fn is_schedule_allowed(hot_operator_count: u64, hot_limit: u64) -> bool {
        hot_operator_count < hot_limit
    }
    /// Call after snapshot preparation, even when the direction is forbidden.
    pub async fn dispatch<'a, V>(
        &mut self,
        resource: HotResource,
        candidates: &[Vec<HotSolveCandidate<'a>>; 4],
        ctx: &mut HotSolveContext<'_, V>,
    ) -> HotDispatchResult<'a> {
        let mut out = HotDispatchResult {
            offers: vec![],
            diagnostic: None,
            errors: vec![],
        };
        if self.config.is_forbidden(if resource.rw() == RWType::READ {
            "read"
        } else {
            "write"
        }) {
            out.diagnostic = Some(HotScheduleDiagnostic::Forbidden);
            return out;
        }
        let resources: &[HotResource] = match resource {
            HotResource::ReadPeer | HotResource::ReadLeader => {
                &[HotResource::ReadLeader, HotResource::ReadPeer]
            }
            HotResource::WritePeer => &[HotResource::WritePeer],
            HotResource::WriteLeader => &[HotResource::WriteLeader],
        };
        for &r in resources {
            let mut result = self.solvers[r as usize]
                .solve(&candidates[r as usize], ctx)
                .await;
            out.errors.append(&mut result.errors);
            if !result.operators.is_empty() {
                out.offers.push((r, result));
            }
        }
        if out.offers.len() == 2
            && !self.solvers[HotResource::ReadLeader as usize]
                .prefers_result(&out.offers[0].1, &out.offers[1].1)
        {
            out.offers.swap(0, 1);
        }
        if out.offers.is_empty() {
            out.diagnostic = Some(HotScheduleDiagnostic::NoSolution);
        }
        out
    }
    pub async fn schedule<'a, V>(
        &mut self,
        resources: &[HotResource],
        random_word: usize,
        candidates: &[Vec<HotSolveCandidate<'a>>; 4],
        ctx: &mut HotSolveContext<'_, V>,
    ) -> Result<HotDispatchResult<'a>, HotScheduleError> {
        if resources.is_empty() {
            return Err(HotScheduleError::EmptyResources);
        }
        Ok(self
            .dispatch(resources[random_word % resources.len()], candidates, ctx)
            .await)
    }
}
