// Copyright 2026 AsterSQL.
//! Candidate comparison and atomic hot peer/reverse operator construction.
use crate::{
    HotDestinationSelector, HotPlacementScope, HotRankV1, HotRankV2, HotRankV2Result,
    HotRegionParams, HotSolution, HotSourceOperation, HotSourceSelector,
    same_hot_placement_population,
};
use astersql_pd_core::{Peer, RegionInfo, StoresInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_operator::{
    BuilderContext, BuilderError, OpKind, Operator, PeerOperatorFactory,
};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{HotPeerStat, StoreLoad, StoreLoadDetail};
use astersql_pd_statistics_utils::RWType;
use std::sync::Arc;

/// Truncate toward zero, including negative intervals, as Go's int64 conversion does.
/// Finite inputs and nonzero step are the scheduler's ordinary domain.
pub fn hot_step_rank(base: f64, step: f64, rate: f64) -> i64 {
    ((rate - base) / step) as i64
}
/// Reverse candidates come from the same round's health/placement peer admission.
#[derive(Clone)]
pub struct HotRevertCandidate<'a> {
    pub peer: &'a HotPeerStat,
    pub region: &'a RegionInfo,
    pub scope: Option<&'a HotPlacementScope>,
}
/// One admitted source/main-peer/destination combination. The scheduler owns
/// source, region, snapshot/affinity and destination filtering (tasks 89/90/94).
/// Optional populations are real task-91 scopes, never caller-invented averages.
#[derive(Clone)]
pub struct HotSolveCandidate<'a> {
    pub source: &'a StoreLoadDetail,
    pub destination: &'a StoreLoadDetail,
    pub main_peer: &'a HotPeerStat,
    pub region: &'a RegionInfo,
    pub revert_peer: Option<&'a HotPeerStat>,
    pub revert_region: Option<&'a RegionInfo>,
    pub scope: Option<&'a HotPlacementScope>,
    pub revert_scope: Option<&'a HotPlacementScope>,
    pub source_top: [Option<f64>; 2],
    pub destination_top: [Option<f64>; 2],
    pub reverts: Vec<HotRevertCandidate<'a>>,
}
impl<'a> HotSolveCandidate<'a> {
    pub fn new(
        source: &'a StoreLoadDetail,
        destination: &'a StoreLoadDetail,
        main_peer: &'a HotPeerStat,
        region: &'a RegionInfo,
    ) -> Self {
        Self {
            source,
            destination,
            main_peer,
            region,
            revert_peer: None,
            revert_region: None,
            scope: None,
            revert_scope: None,
            source_top: [None; 2],
            destination_top: [None; 2],
            reverts: vec![],
        }
    }
    /// References encode Go's non-nil checks; stale peer/region identities remain checked.
    pub fn is_ready_to_build(&self) -> bool {
        self.main_peer.store_id == self.source.store_summary_info.store_info.id()
            && self.main_peer.region_id == self.region.meta().id
            && match (self.revert_peer, self.revert_region) {
                (None, None) => true,
                (Some(p), Some(r)) => {
                    p.store_id == self.destination.store_summary_info.store_info.id()
                        && p.region_id == r.meta().id
                }
                _ => false,
            }
    }
    fn solution(&self) -> HotSolution<'a> {
        HotSolution::new(
            self.source,
            self.destination,
            self.main_peer,
            self.revert_peer,
        )
    }
}
/// Host-owned real builder dependencies. Allocation is the only external write.
pub struct HotSolveContext<'a, V> {
    pub stores: &'a StoresInfo,
    pub config: &'a dyn SharedConfigProvider<Version = V>,
    pub rules: Option<&'a RuleManager>,
    pub now_unix_nanos: i128,
    pub supports_conf_change_v2: bool,
    pub allocate: &'a mut dyn FnMut() -> Result<u64, BuilderError>,
}
/// Metric labels returned to the host, which owns counter registration/lifecycle.
#[derive(Debug, PartialEq, Eq)]
pub struct HotOperatorDecoration {
    pub operation: &'static str,
    pub rw: &'static str,
    pub dimension: String,
    pub directions: Vec<(u64, &'static str)>,
    pub counters: [&'static str; 2],
}
#[derive(Default)]
pub struct HotSolveResult<'a> {
    pub operators: Vec<Arc<Operator>>,
    pub decorations: Vec<HotOperatorDecoration>,
    pub best: Option<HotSolveCandidate<'a>>,
    pub best_rank: Option<i32>,
    pub best_revert: bool,
    /// Rejected builds do not erase the last executable winner.
    pub errors: Vec<BuilderError>,
}
enum Formula {
    V1(HotRankV1),
    V2(HotRankV2),
}
/// Stateful search flag survives rounds; all loads and candidates are borrowed
/// from a single host snapshot. Bucket split decisions are owned by task 93.
pub struct HotSolver {
    formula: Formula,
    order: HotRankV1,
    source: HotSourceSelector,
    destination: HotDestinationSelector,
    rw: RWType,
    operation: HotSourceOperation,
    priorities: [usize; 2],
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotSolveError {
    #[error("hot solver requires two distinct dimensions in 0..4")]
    InvalidPriorities,
    #[error("hot solver peer limit must be nonnegative")]
    InvalidPeerLimit,
}
impl HotSolver {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        rw: RWType,
        operation: HotSourceOperation,
        max_src: StoreLoad,
        min_dst: StoreLoad,
        step: StoreLoad,
    ) -> Result<Self, HotSolveError> {
        if priorities.iter().any(|&d| d >= 4) || priorities[0] == priorities[1] {
            return Err(HotSolveError::InvalidPriorities);
        }
        if config.max_peer_number < 0 {
            return Err(HotSolveError::InvalidPeerLimit);
        }
        let write = rw == RWType::WRITE && operation == HotSourceOperation::TransferLeader;
        let order = HotRankV1::new(
            config.clone(),
            priorities,
            write,
            max_src.clone(),
            min_dst.clone(),
            step.clone(),
        )
        .map_err(|_| HotSolveError::InvalidPriorities)?;
        let formula = if config.rank_formula_version == "v1" {
            Formula::V1(
                HotRankV1::new(config.clone(), priorities, write, max_src, min_dst, step)
                    .map_err(|_| HotSolveError::InvalidPriorities)?,
            )
        } else {
            Formula::V2(
                HotRankV2::new(config.clone(), priorities, write, max_src, min_dst, step)
                    .map_err(|_| HotSolveError::InvalidPriorities)?,
            )
        };
        Ok(Self {
            formula,
            order,
            source: HotSourceSelector::new(config.clone(), priorities, rw, operation)
                .map_err(|_| HotSolveError::InvalidPeerLimit)?,
            destination: HotDestinationSelector::new(config, priorities, rw, operation)
                .map_err(|_| HotSolveError::InvalidPriorities)?,
            rw,
            operation,
            priorities,
        })
    }
    pub(crate) fn refresh_rank_inputs(
        &mut self,
        input: crate::HotRankInputs,
    ) -> Result<(), HotSolveError> {
        let config = self.source.config_snapshot().clone();
        let mut fresh = Self::new(
            config,
            self.priorities,
            self.rw,
            self.operation,
            input.max_source,
            input.min_destination,
            input.step,
        )?;
        if let (Formula::V2(old), Formula::V2(new)) = (&self.formula, &mut fresh.formula) {
            new.search_revert_regions = old.search_revert_regions;
        }
        self.order = fresh.order;
        self.formula = fresh.formula;
        Ok(())
    }
    pub fn compare_src(&self, a: &StoreLoadDetail, b: &StoreLoadDetail) -> i32 {
        self.order.compare_store(a, b, true)
    }
    pub fn compare_dst(&self, a: &StoreLoadDetail, b: &StoreLoadDetail) -> i32 {
        self.order.compare_store(a, b, false)
    }
    pub fn set_search_revert_regions(&mut self, best: Option<(i32, bool)>) {
        if let Formula::V2(r) = &mut self.formula {
            r.set_search_revert_regions(best);
        }
    }
    fn rank(&self, c: &HotSolveCandidate<'_>, s: &mut HotSolution<'_>) -> HotRankV2Result {
        match &self.formula {
            Formula::V1(r) => HotRankV2Result {
                rank: r.progressive_rank(s),
                scores: [0; 2],
            },
            Formula::V2(r) => r.progressive_rank(s, c.source_top, c.destination_top),
        }
    }
    fn search(&self, current: i32, best: Option<(i32, bool)>) -> bool {
        match &self.formula {
            Formula::V1(_) => false,
            Formula::V2(r) => r.need_search_revert_regions(current, best),
        }
    }
    fn eligible(&self, c: &HotSolveCandidate<'_>, rank: HotRankV2Result) -> bool {
        match &self.formula {
            Formula::V1(r) => {
                r.is_available(rank.rank) && r.filter_uniform_store(c.source, rank.rank).is_none()
            }
            Formula::V2(r) => {
                rank.is_available(c.revert_peer.is_some())
                    && r.filter_uniform_store(
                        c.source,
                        rank.rank,
                        c.scope.map(|s| &s.stddev),
                        c.revert_scope.map(|s| &s.stddev),
                        c.revert_peer.is_some(),
                    )
                    .is_none()
            }
        }
    }
    fn better(
        &self,
        c: &HotSolveCandidate<'_>,
        rank: HotRankV2Result,
        best: Option<(&HotSolveCandidate<'_>, HotRankV2Result)>,
    ) -> bool {
        let mut s = c.solution();
        s.calc_peers_rate(&self.priorities);
        let old = best.map(|(b, r)| {
            let mut s = b.solution();
            s.calc_peers_rate(&self.priorities);
            (s, r)
        });
        match &self.formula {
            Formula::V1(r) => r.better_than(&s, rank.rank, old.as_ref().map(|(s, r)| (s, r.rank))),
            Formula::V2(r) => r.better_than(&s, rank, old.as_ref().map(|(s, r)| (s, *r))),
        }
    }
    /// Visit main candidates before each reverse candidate. Commit a best solution
    /// only after every operator in its pair is built successfully. No operators
    /// are submitted and no pending accounting is modified here.
    pub async fn solve<'a, V>(
        &mut self,
        candidates: &[HotSolveCandidate<'a>],
        ctx: &mut HotSolveContext<'_, V>,
    ) -> HotSolveResult<'a> {
        let mut out = HotSolveResult::default();
        let mut best_result = HotRankV2Result::default();
        for main in candidates {
            let mut forward = main.clone();
            forward.revert_peer = None;
            forward.revert_region = None;
            forward.revert_scope = None;
            let rank = self.rank(&forward, &mut forward.solution());
            self.try_candidate(forward.clone(), rank, &mut best_result, &mut out, ctx)
                .await;
            if !self.search(rank.rank, out.best_rank.map(|r| (r, out.best_revert))) {
                continue;
            }
            for back in &main.reverts {
                if back.region.meta().id == main.region.meta().id
                    || !self.destination.allow_revert_region(
                        back.region,
                        main.source.store_summary_info.store_info.id(),
                    )
                {
                    continue;
                }
                let mut reverse = forward.clone();
                reverse.revert_peer = Some(back.peer);
                reverse.revert_region = Some(back.region);
                reverse.revert_scope = back.scope;
                if !same_hot_placement_population(
                    main.scope,
                    main.source.store_summary_info.store_info.is_tikv(),
                    back.scope,
                    main.destination.store_summary_info.store_info.is_tikv(),
                ) && (!self
                    .source
                    .source_store_decision(main.destination, back.scope.map(|s| &s.expect))
                    .admitted()
                    || !self
                        .destination
                        .destination_store_decision(
                            main.source,
                            back.scope.map(|s| s.destination_scope()).as_ref(),
                        )
                        .admitted())
                {
                    continue;
                }
                let rank = self.rank(&reverse, &mut reverse.solution());
                self.try_candidate(reverse, rank, &mut best_result, &mut out, ctx)
                    .await;
            }
        }
        self.set_search_revert_regions(out.best_rank.map(|r| (r, out.best_revert)));
        out
    }
    async fn try_candidate<'a, V>(
        &self,
        c: HotSolveCandidate<'a>,
        rank: HotRankV2Result,
        best_result: &mut HotRankV2Result,
        out: &mut HotSolveResult<'a>,
        ctx: &mut HotSolveContext<'_, V>,
    ) {
        if !c.is_ready_to_build()
            || !self.eligible(&c, rank)
            || !self.better(&c, rank, out.best.as_ref().map(|b| (b, *best_result)))
        {
            return;
        }
        match self.build_operators(&c, rank.rank, ctx).await {
            Ok((operators, decorations)) if !operators.is_empty() => {
                out.operators = operators;
                out.decorations = decorations;
                out.best_rank = Some(rank.rank);
                out.best_revert = c.revert_peer.is_some();
                out.best = Some(c);
                *best_result = rank;
            }
            Err(error) => out.errors.push(error),
            _ => {}
        }
    }
    /// Preserve peer role. Existing destinations transfer leadership; absent
    /// destinations move a leader or a peer using the canonical real planner.
    pub async fn build_operators<V>(
        &self,
        c: &HotSolveCandidate<'_>,
        rank: i32,
        ctx: &mut HotSolveContext<'_, V>,
    ) -> Result<(Vec<Arc<Operator>>, Vec<HotOperatorDecoration>), BuilderError> {
        if !c.is_ready_to_build() {
            return Err(BuilderError::InvalidOriginPeer);
        }
        let src = c.source.store_summary_info.store_info.id();
        let dst = c.destination.store_summary_info.store_info.id();
        let mut ops = vec![];
        let mut decorations = vec![];
        for (region, from, to, revert) in [
            (Some(c.region), src, dst, false),
            (c.revert_region, dst, src, true),
        ] {
            let Some(region) = region else {
                continue;
            };
            let typ = if region.get_store_peer(to).is_some() {
                "transfer-leader"
            } else if region.leader().is_some_and(|p| p.store_id == from) {
                "move-leader"
            } else {
                "move-peer"
            };
            let rw = if self.rw == RWType::READ {
                "read"
            } else {
                "write"
            };
            let rules = ctx
                .rules
                .map(|m| m.get_rules_for_apply_region(region))
                .unwrap_or_default();
            let description = format!(
                "{}-hot-{}-{}",
                if typ == "transfer-leader" {
                    "transfer"
                } else {
                    "move"
                },
                rw,
                if typ == "move-peer" { "peer" } else { "leader" }
            );
            let factory = PeerOperatorFactory::new(
                &description,
                BuilderContext {
                    stores: ctx.stores,
                    config: ctx.config,
                    rules: &rules,
                    now_unix_nanos: ctx.now_unix_nanos,
                },
                region,
                ctx.supports_conf_change_v2,
            );
            let op = if typ == "transfer-leader" {
                factory
                    .create_transfer_leader(to, vec![], OpKind::HOT_REGION, &mut *ctx.allocate)
                    .await?
            } else {
                let old = region
                    .get_store_peer(from)
                    .ok_or(BuilderError::MissingPeer(from))?;
                let peer = Some(Peer {
                    store_id: to,
                    role: old.role,
                    ..Default::default()
                });
                if typ == "move-leader" {
                    factory
                        .create_move_leader(from, peer, OpKind::HOT_REGION, &mut *ctx.allocate)
                        .await?
                } else {
                    factory
                        .create_move_peer(from, peer, OpKind::HOT_REGION, &mut *ctx.allocate)
                        .await?
                }
            };
            op.set_store_health_check(true);
            op.set_priority_level(PriorityLevel::HIGH);
            let dimension = match &self.formula {
                Formula::V1(r) => r.rank_to_dim_string(rank),
                Formula::V2(r) => r.rank_to_dim_string(rank),
            };
            let mut directions = vec![(from, "out"), (to, "in")];
            if revert {
                directions.extend([(from, "out-for-revert"), (to, "in-for-revert")]);
            }
            decorations.push(HotOperatorDecoration {
                operation: typ,
                rw,
                dimension,
                directions,
                counters: ["new-operator", typ],
            });
            ops.push(Arc::new(op));
        }
        Ok((ops, decorations))
    }
    /// Compare read-leader and read-peer winners using the leader's rank formula,
    /// as Go balanceHotReadRegions does. Both results retain real candidates.
    pub fn prefers_result(&self, current: &HotSolveResult<'_>, old: &HotSolveResult<'_>) -> bool {
        let Some(c) = current.best.as_ref() else {
            return false;
        };
        let Some(b) = old.best.as_ref() else {
            return true;
        };
        let rank = self.rank(c, &mut c.solution());
        let old_rank = self.rank(b, &mut b.solution());
        self.better(c, rank, Some((b, old_rank)))
    }
    pub fn operation(&self) -> HotSourceOperation {
        self.operation
    }
}
