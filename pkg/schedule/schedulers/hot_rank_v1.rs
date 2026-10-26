// Copyright 2026 AsterSQL.
//! First hot ranking formula over canonical store predictions and peer projections.
use crate::{HotRegionParams, HotSolution, rank_cmp};
use astersql_pd_statistics::{StoreLoad, StoreLoadDetail};

/// Rejected priority dimensions; callers must resolve configuration priorities first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotRankV1Error {
    InvalidPriorities,
}
impl std::fmt::Display for HotRankV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("hot rank v1 requires two distinct dimensions in 0..4")
    }
}
impl std::error::Error for HotRankV1Error {}

/// Per-round rank context. Population extrema and rank steps belong to the caller's
/// eligible store snapshot. Config is a snapshot, with no global state or I/O.
pub struct HotRankV1 {
    pub config: HotRegionParams,
    priorities: [usize; 2],
    write_leader: bool,
    max_src: StoreLoad,
    min_dst: StoreLoad,
    rank_step: StoreLoad,
}
impl HotRankV1 {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        write_leader: bool,
        max_src: StoreLoad,
        min_dst: StoreLoad,
        rank_step: StoreLoad,
    ) -> Result<Self, HotRankV1Error> {
        if priorities.iter().any(|&d| d >= 4) || priorities[0] == priorities[1] {
            return Err(HotRankV1Error::InvalidPriorities);
        }
        Ok(Self {
            config,
            priorities,
            write_leader,
            max_src,
            min_dst,
            rank_step,
        })
    }
    fn check_priorities(&self, mut f: impl FnMut(usize) -> bool) -> bool {
        if self.write_leader || !self.config.strict_picking_store {
            f(self.priorities[0])
        } else {
            // Go AllOf visits dimensions in array order, not configured priority order.
            (0..4).filter(|d| self.priorities.contains(d)).all(f)
        }
    }
    pub fn check_loads(
        &self,
        load: &StoreLoad,
        expect: &StoreLoad,
        tolerance: f64,
        source: bool,
    ) -> bool {
        self.check_priorities(|d| {
            if source {
                load.loads[d] > tolerance * expect.loads[d]
            } else {
                load.loads[d] * tolerance < expect.loads[d]
            }
        })
    }
    /// Empty dimension history passes; a shorter expectation panics on indexing as in Go.
    pub fn check_history(
        &self,
        load: &StoreLoad,
        expect: &StoreLoad,
        tolerance: f64,
        source: bool,
    ) -> bool {
        self.check_priorities(|d| {
            load.history_loads[d].iter().enumerate().all(|(i, &value)| {
                if source {
                    value > tolerance * expect.history_loads[d][i]
                } else {
                    value * tolerance < expect.history_loads[d][i]
                }
            })
        })
    }
    pub fn is_available(&self, rank: i32) -> bool {
        rank > 0
    }
    pub fn need_search_revert_regions(&self) -> bool {
        false
    }
    /// V1 never enables revert search, even when asked to update its search state.
    pub fn set_search_revert_regions(&self) {}
    fn min_rate(&self, dim: usize) -> f64 {
        [
            self.config.min_hot_byte_rate,
            self.config.min_hot_key_rate,
            self.config.min_hot_query_rate,
            self.config.min_hot_cpu_rate,
        ][dim]
    }
    /// Pending amplification uses current loads, independently of signed net peer rate.
    pub fn is_tolerance(&self, solution: &HotSolution<'_>, dim: usize) -> bool {
        let (src, dst) = solution.current_load(dim);
        if src <= dst {
            return false;
        }
        let (sp, dp) = solution.pending_load(dim);
        let amplification = 1. + 2. * src / (src - dst);
        src - amplification * sp > dst + amplification * dp
    }
    pub fn hot_decline_ratio(&self, solution: &HotSolution<'_>, dim: usize) -> (bool, f64) {
        let (src, dst) = solution.extreme_load(dim);
        let peer = solution.peers_rate(dim);
        let denominator = if peer >= 0. { src - peer } else { dst + peer };
        // Go math.Max propagates NaN rather than silently replacing it with one.
        let denominator = if denominator.is_nan() {
            denominator
        } else {
            denominator.max(1.)
        };
        (
            peer.abs() >= self.min_rate(dim),
            if peer >= 0. {
                (dst + peer) / denominator
            } else {
                (src - peer) / denominator
            },
        )
    }
    pub fn progressive_rank(&self, solution: &mut HotSolution<'_>) -> i32 {
        let [first, second] = self.priorities;
        solution.calc_peers_rate(&self.priorities);
        if solution.peers_rate(first) < self.min_rate(first)
            && solution.peers_rate(second) < self.min_rate(second)
        {
            return -1;
        }
        if self.write_leader {
            let (src, dst) = solution.extreme_load(first);
            let peer = solution.peers_rate(first);
            return if src - peer >= dst + peer && self.is_tolerance(solution, first) {
                3
            } else {
                -1
            };
        }
        let state = |d| {
            let (hot, ratio) = self.hot_decline_ratio(solution, d);
            let better =
                hot && ratio <= self.config.great_dec_ratio && self.is_tolerance(solution, d);
            (
                better,
                better || !hot || ratio <= self.config.minor_dec_ratio,
            )
        };
        let (fb, fnw) = state(first);
        let (sb, snw) = state(second);
        match (fb, fnw, sb, snw) {
            (true, _, true, _) => 4,
            (true, _, _, true) => 3,
            (_, true, true, _) => 2,
            (true, _, _, _) => 1,
            (_, _, true, _) => 0,
            _ => -1,
        }
    }
    /// V1 uses the global population's normalized standard deviation. Placement
    /// scope restriction is a v2 solver policy and is not enabled for this formula.
    pub fn filter_uniform_store(
        &self,
        source: &StoreLoadDetail,
        rank: i32,
    ) -> Option<&'static str> {
        if !(self.config.src_tolerance_ratio > 0. && self.config.dst_tolerance_ratio > 0.) {
            return None;
        }
        let [first, second] = self.priorities;
        let a = source.is_uniform(first, 0.05);
        let b = source.is_uniform(second, 0.1);
        if a && b {
            Some("all-dim")
        } else if a && (rank == 1 || rank == 3) {
            Some(dim_name(first))
        } else if b && rank == 2 {
            Some(dim_name(second))
        } else {
            None
        }
    }
    pub fn rank_to_dim_string(&self, rank: i32) -> String {
        let [first, second] = self.priorities;
        match rank {
            4 => "all".into(),
            3 => dim_name(first).into(),
            2 => dim_name(second).into(),
            1 => format!("{}-only", dim_name(first)),
            _ => "none".into(),
        }
    }
    pub(crate) fn compare_store(
        &self,
        a: &StoreLoadDetail,
        b: &StoreLoadDetail,
        source: bool,
    ) -> i32 {
        if std::ptr::eq(a, b) {
            return 0;
        }
        let ap = &a.load_pred;
        let bp = &b.load_pred;
        let (av, bv, base) = if source {
            (ap.min(), bp.min(), &self.max_src)
        } else {
            (ap.max(), bp.max(), &self.min_dst)
        };
        for d in self.priorities {
            let cmp = step_cmp(
                av.loads[d],
                bv.loads[d],
                base.loads[d],
                self.rank_step.loads[d],
            );
            if cmp != 0 {
                return if source { -cmp } else { cmp };
            }
        }
        let ad = ap.diff();
        let bd = bp.diff();
        if self.write_leader {
            let cmp = step_cmp(
                ad.hot_peer_count,
                bd.hot_peer_count,
                0.,
                self.rank_step.hot_peer_count,
            );
            if cmp != 0 {
                return cmp;
            }
        }
        for d in self
            .priorities
            .into_iter()
            .take(if self.write_leader { 2 } else { 1 })
        {
            let cmp = step_cmp(ad.loads[d], bd.loads[d], 0., self.rank_step.loads[d]);
            if cmp != 0 {
                return cmp;
            }
        }
        0
    }
    /// Lexicographic preference: rank, revert count, source, destination, peer rates.
    /// Split rank >=5 always wins; identical candidates retain the previous winner.
    pub fn better_than(
        &self,
        current: &HotSolution<'_>,
        rank: i32,
        old: Option<(&HotSolution<'_>, i32)>,
    ) -> bool {
        let Some((old, old_rank)) = old else {
            return true;
        };
        if rank >= 5 {
            return true;
        }
        if rank != old_rank {
            return rank > old_rank;
        }
        if current.revert_peer.is_some() != old.revert_peer.is_some() {
            return current.revert_peer.is_none();
        }
        for (a, b, source) in [
            (current.src_store, old.src_store, true),
            (current.dst_store, old.dst_store, false),
        ] {
            let cmp = self.compare_store(a, b, source);
            if cmp != 0 {
                return cmp < 0;
            }
        }
        if std::ptr::eq(current.main_peer, old.main_peer) {
            return false;
        }
        let [first, second] = self.priorities;
        if self.write_leader {
            return current.peers_rate(first) > old.peers_rate(first);
        }
        let cmp = |d| {
            step_cmp(
                current.peers_rate(d),
                old.peers_rate(d),
                0.,
                [100., 10., 10., 10.][d],
            )
        };
        let (a, b) = (cmp(first), cmp(second));
        match rank {
            4 => {
                if a != 0 {
                    a > 0
                } else {
                    b > 0
                }
            }
            3 => {
                if a != 0 {
                    a > 0
                } else {
                    b < 0
                }
            }
            2 => {
                if b != 0 {
                    b > 0
                } else {
                    a < 0
                }
            }
            1 => a > 0,
            _ => false,
        }
    }
}
fn step_cmp(a: f64, b: f64, base: f64, step: f64) -> i32 {
    rank_cmp(a, b, |rate| ((rate - base) / step) as i64)
}
fn dim_name(dim: usize) -> &'static str {
    ["byte", "key", "query", "cpu"][dim]
}
