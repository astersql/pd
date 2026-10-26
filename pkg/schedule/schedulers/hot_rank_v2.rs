// Copyright 2026 AsterSQL.
//! Second hot rank formula, borrowing canonical predictions and pending projections.
use crate::{HotRankV1, HotRegionParams, HotSolution, rank_cmp};
use astersql_pd_statistics::{StoreLoad, StoreLoadDetail};

/// Invalid selected dimensions, rejected before accessing load arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotRankV2Error {
    InvalidPriorities,
}
impl std::fmt::Display for HotRankV2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("hot rank v2 requires two distinct dimensions in 0..4")
    }
}
impl std::error::Error for HotRankV2Error {}

/// Current and future balance thresholds for one selected dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RankRatios {
    pub future_balanced: f64,
    pub future_pre_balanced: f64,
    pub current_balanced: f64,
    pub current_pre_balanced: f64,
    pub perceived: f64,
    pub min_hot: f64,
}
impl RankRatios {
    pub fn new(mut balanced: f64, perceived: f64, min_hot: f64) -> Self {
        // Ordered comparisons preserve Go's NaN behavior.
        if balanced < 0.7 {
            balanced = 0.7;
        }
        if balanced > 0.95 {
            balanced = 0.95;
        }
        let pre = (2. * balanced - 1.).max(balanced - 0.1);
        Self {
            future_balanced: balanced,
            future_pre_balanced: pre,
            current_balanced: balanced - 0.02,
            current_pre_balanced: pre - 0.03,
            perceived,
            min_hot,
        }
    }
}
/// Scores are kept with the rank, independently of the peer-rate cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HotRankV2Result {
    pub rank: i32,
    pub scores: [i32; 2],
}
impl HotRankV2Result {
    pub fn is_available(self, revert: bool) -> bool {
        self.rank >= 3 || self.rank > 0 && !revert
    }
}
/// Per-round ranking context. Eligible store extrema/steps and top-n peer rates
/// come from the solver snapshot. This library performs no I/O or global mutation.
pub struct HotRankV2 {
    pub config: HotRegionParams,
    priorities: [usize; 2],
    write_leader: bool,
    store_order: HotRankV1,
    pub ratios: [RankRatios; 2],
    pub search_revert_regions: bool,
}
impl HotRankV2 {
    pub fn new(
        config: HotRegionParams,
        priorities: [usize; 2],
        write_leader: bool,
        max_src: StoreLoad,
        min_dst: StoreLoad,
        step: StoreLoad,
    ) -> Result<Self, HotRankV2Error> {
        let store_order = HotRankV1::new(
            config.clone(),
            priorities,
            write_leader,
            max_src,
            min_dst,
            step,
        )
        .map_err(|_| HotRankV2Error::InvalidPriorities)?;
        let first = RankRatios::new(config.great_dec_ratio, 0.2, 0.02);
        Ok(Self {
            config,
            priorities,
            write_leader,
            store_order,
            ratios: [first, RankRatios::new(first.future_pre_balanced, 0.3, 0.03)],
            search_revert_regions: false,
        })
    }
    fn min_rate(&self, d: usize) -> f64 {
        [
            self.config.min_hot_byte_rate,
            self.config.min_hot_key_rate,
            self.config.min_hot_query_rate,
            self.config.min_hot_cpu_rate,
        ][d]
    }
    fn any_priority(&self, mut f: impl FnMut(usize) -> bool) -> bool {
        if self.write_leader {
            f(self.priorities[0])
        } else {
            (0..4).filter(|d| self.priorities.contains(d)).any(f)
        }
    }
    pub fn check_loads(
        &self,
        load: &StoreLoad,
        expect: &StoreLoad,
        tolerance: f64,
        source: bool,
    ) -> bool {
        self.any_priority(|d| {
            if source {
                load.loads[d] > tolerance * expect.loads[d]
            } else {
                load.loads[d] * tolerance < expect.loads[d]
            }
        })
    }
    /// Samples are aligned by the owner; short expectation history panics like Go.
    pub fn check_history(
        &self,
        load: &StoreLoad,
        expect: &StoreLoad,
        tolerance: f64,
        source: bool,
    ) -> bool {
        self.any_priority(|d| {
            load.history_loads[d].iter().enumerate().all(|(i, &v)| {
                if source {
                    v > tolerance * expect.history_loads[d][i]
                } else {
                    v * tolerance < expect.history_loads[d][i]
                }
            })
        })
    }
    pub fn is_tolerance(&self, s: &HotSolution<'_>, d: usize, reverse: bool) -> bool {
        let (mut src, mut dst) = s.current_load(d);
        let (mut sp, mut dp) = s.pending_load(d);
        if reverse {
            std::mem::swap(&mut src, &mut dst);
            std::mem::swap(&mut sp, &mut dp);
        }
        if src <= dst {
            return false;
        }
        let amp = 1. + 2. * src / (src - dst);
        src - amp * sp > dst + amp * dp
    }
    /// Score one dimension. top_n is the selected high store's nth hot peer rate;
    /// absence means Go's MaxFloat64. Cache must already contain this dimension.
    pub fn score(&self, s: &HotSolution<'_>, d: usize, rs: RankRatios, top_n: Option<f64>) -> i32 {
        let (src, dst) = s.extreme_load(d);
        let (sp, dp) = s.pending_load(d);
        let reverse = src < dst;
        let (high, low, p) = if reverse {
            (dst, src, -s.peers_rate(d))
        } else {
            (src, dst, s.peers_rate(d))
        };
        let balanced_min = (high * rs.future_balanced - low) / (1. + rs.future_balanced);
        let balanced_max = (high - low * rs.future_balanced) / (1. + rs.future_balanced);
        let min = self.min_rate(d);
        if high * rs.current_balanced <= low {
            let lower = if balanced_min > -min {
                -min
            } else {
                balanced_min
            };
            return if p >= lower && p <= balanced_max {
                0
            } else {
                -2
            };
        }
        let pre = high * rs.current_pre_balanced <= low;
        let (nw_min, better_min, better_max, nw_max) = if pre {
            let lower = (high * rs.future_pre_balanced - low) / (1. + rs.future_pre_balanced);
            (
                if lower > -min { -min } else { lower },
                balanced_min,
                balanced_max,
                (high - low * rs.future_pre_balanced) / (1. + rs.future_pre_balanced),
            )
        } else {
            let lower = go_min(
                go_min(balanced_min * rs.perceived, low * rs.min_hot),
                top_n.unwrap_or(f64::MAX),
            );
            (
                -min,
                lower,
                balanced_max + rs.perceived * (high - low - balanced_max - lower),
                balanced_max + rs.perceived * (high - low - balanced_max),
            )
        };
        if better_min <= p && p <= better_max {
            if p >= min && self.is_tolerance(s, d, reverse) && (!pre || sp.abs() + dp.abs() < 1.) {
                if p < balanced_min {
                    2
                } else if p > balanced_max {
                    1
                } else {
                    3
                }
            } else {
                0
            }
        } else if nw_min <= p && p < better_min {
            0
        } else if better_max < p && p <= nw_max {
            -1
        } else {
            -2
        }
    }
    /// src_top/dst_top use priority order, with reverse load selecting dst_top.
    pub fn progressive_rank(
        &self,
        s: &mut HotSolution<'_>,
        src_top: [Option<f64>; 2],
        dst_top: [Option<f64>; 2],
    ) -> HotRankV2Result {
        s.calc_peers_rate(&self.priorities);
        let [first, second] = self.priorities;
        let mut result = HotRankV2Result {
            rank: -1,
            scores: [0; 2],
        };
        if s.peers_rate(first) < self.min_rate(first)
            && s.peers_rate(second) < self.min_rate(second)
        {
            return result;
        }
        let score = |i: usize| {
            let d = self.priorities[i];
            let (src, dst) = s.extreme_load(d);
            self.score(
                s,
                d,
                self.ratios[i],
                if src < dst { dst_top[i] } else { src_top[i] },
            )
        };
        if self.write_leader {
            if score(0) > 0 {
                result.rank = 3;
            }
            return result;
        }
        let a = score(0);
        let b = score(1);
        result.scores = [a, b];
        result.rank = match (a, b) {
            (1.., 1..) => 4,
            (1.., 0) => 3,
            (1.., _) => 2,
            (0, 1..) => 1,
            (_, 1..) => 0,
            _ => -1,
        };
        result
    }
    pub fn need_search_revert_regions(&self, current: i32, best: Option<(i32, bool)>) -> bool {
        self.search_revert_regions
            && (current == 2 || current == 0)
            && best.is_none_or(|(r, revert)| r <= 2 || revert)
    }
    pub fn set_search_revert_regions(&mut self, best: Option<(i32, bool)>) {
        self.search_revert_regions = best.is_none_or(|(r, revert)| r == 2 || revert);
    }
    /// Scopes contain the solver's real population statistics; None falls back
    /// to the global source. A revert scope is consulted only for a revert candidate.
    pub fn filter_uniform_store(
        &self,
        source: &StoreLoadDetail,
        rank: i32,
        scope: Option<&StoreLoad>,
        revert_scope: Option<&StoreLoad>,
        revert: bool,
    ) -> Option<&'static str> {
        if !(self.config.src_tolerance_ratio > 0. && self.config.dst_tolerance_ratio > 0.) {
            return None;
        }
        let uniform = |d, threshold| {
            let in_scope = |s: Option<&StoreLoad>| {
                s.map_or_else(
                    || source.is_uniform(d, threshold),
                    |s| s.loads[d] < threshold,
                )
            };
            in_scope(scope) || revert && in_scope(revert_scope)
        };
        let [a, b] = self.priorities;
        let x = uniform(a, 0.05);
        let y = uniform(b, 0.1);
        if x && y {
            Some("all-dim")
        } else if x && (rank == 2 || rank == 3) {
            Some(dim_name(a))
        } else if y && rank == 1 {
            Some(dim_name(b))
        } else {
            None
        }
    }
    pub fn rank_to_dim_string(&self, rank: i32) -> String {
        match rank {
            4 => "all".into(),
            3 => dim_name(self.priorities[0]).into(),
            2 => format!("{}-only", dim_name(self.priorities[0])),
            1 => dim_name(self.priorities[1]).into(),
            _ => "none".into(),
        }
    }
    pub fn better_than(
        &self,
        current: &HotSolution<'_>,
        result: HotRankV2Result,
        old: Option<(&HotSolution<'_>, HotRankV2Result)>,
    ) -> bool {
        let Some((old, previous)) = old else {
            return true;
        };
        if result.rank >= 5 {
            return true;
        }
        if result.rank != previous.rank {
            return result.rank > previous.rank;
        }
        if current.revert_peer.is_some() != old.revert_peer.is_some() {
            return current.revert_peer.is_none();
        }
        for (a, b, src) in [
            (current.src_store, old.src_store, true),
            (current.dst_store, old.dst_store, false),
        ] {
            let cmp = self.store_order.compare_store(a, b, src);
            if cmp != 0 {
                return cmp < 0;
            }
        }
        if std::ptr::eq(current.main_peer, old.main_peer) {
            return false;
        }
        let cmp = |i| {
            let d = self.priorities[i];
            v2_priority_cmp(
                d,
                result.scores[i],
                previous.scores[i],
                current.peers_rate(d),
                old.peers_rate(d),
            )
        };
        if self.write_leader {
            return cmp(0) > 0;
        }
        let order = match result.rank {
            2..=4 => [0, 1],
            1 => [1, 0],
            _ => return false,
        };
        let first = cmp(order[0]);
        if first != 0 {
            first > 0
        } else {
            cmp(order[1]) > 0
        }
    }
}
/// Prefer higher score; at score 2 prefer larger bucket, otherwise smaller bucket.
pub fn v2_priority_cmp(dim: usize, score: i32, old_score: i32, rate: f64, old_rate: f64) -> i32 {
    if score != old_score {
        return if score > old_score { 1 } else { -1 };
    }
    let cmp = rank_cmp(rate, old_rate, |v| (v / [100., 10., 10., 10.][dim]) as i64);
    if score == 2 { cmp } else { -cmp }
}
fn dim_name(dim: usize) -> &'static str {
    ["byte", "key", "query", "cpu"][dim]
}

fn go_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
