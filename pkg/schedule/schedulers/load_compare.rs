// Copyright 2026 AsterSQL.
//! Load ordering and pending influence using canonical statistics/operator state.
use astersql_pd_core::{BasicCluster, StoreControlError};
use astersql_pd_core_constant::Direction;
use astersql_pd_schedule_operator::{Clock, Operator, SUCCESS};
use astersql_pd_statistics::{Influence, StoreLoad, StoreLoadPred};
use std::{collections::HashSet, sync::Arc, time::Duration};

/// Signed comparator result; zero delegates to the next criterion.
pub type StoreLoadCmp = Box<dyn Fn(&StoreLoad, &StoreLoad) -> i32 + Send + Sync>;
/// Comparator over current/future predictions.
pub type StoreLPCmp = Box<dyn Fn(&StoreLoadPred, &StoreLoadPred) -> i32 + Send + Sync>;

/// Compare caller-defined discrete ranks, retaining truncation policy at the caller.
pub fn rank_cmp(a: f64, b: f64, rank: impl Fn(f64) -> i64) -> i32 {
    match rank(a).cmp(&rank(b)) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
/// Project a rate or peer count before ranking.
pub fn st_ld_rank_cmp(
    dimension: impl Fn(&StoreLoad) -> f64 + Send + Sync + 'static,
    rank: impl Fn(f64) -> i64 + Send + Sync + 'static,
) -> StoreLoadCmp {
    Box::new(move |a, b| rank_cmp(dimension(a), dimension(b), &rank))
}
/// Reverse a comparison, including custom non-unit signed results.
pub fn neg_load_cmp(cmp: StoreLoadCmp) -> StoreLoadCmp {
    Box::new(move |a, b| cmp(a, b).wrapping_neg())
}
/// Lexicographic load comparison in caller order; an empty list is equal.
pub fn slice_load_cmp(cmps: Vec<StoreLoadCmp>) -> StoreLoadCmp {
    Box::new(move |a, b| {
        cmps.iter()
            .map(|cmp| cmp(a, b))
            .find(|r| *r != 0)
            .unwrap_or(0)
    })
}
/// Lexicographic prediction comparison in caller order.
pub fn slice_lp_cmp(cmps: Vec<StoreLPCmp>) -> StoreLPCmp {
    Box::new(move |a, b| {
        cmps.iter()
            .map(|cmp| cmp(a, b))
            .find(|r| *r != 0)
            .unwrap_or(0)
    })
}
/// Compare component-wise minimum current/future values.
pub fn min_lp_cmp(cmp: StoreLoadCmp) -> StoreLPCmp {
    Box::new(move |a, b| cmp(&a.min(), &b.min()))
}
/// Compare component-wise maximum current/future values.
pub fn max_lp_cmp(cmp: StoreLoadCmp) -> StoreLPCmp {
    Box::new(move |a, b| cmp(&a.max(), &b.max()))
}
/// Compare absolute component-wise changes, including peer count.
pub fn diff_cmp(cmp: StoreLoadCmp) -> StoreLPCmp {
    Box::new(move |a, b| cmp(&a.diff(), &b.diff()))
}

/// Pending record retains the real operator so later lifecycle transitions are visible.
pub struct PendingInfluence {
    pub op: Arc<Operator>,
    pub froms: Vec<u64>,
    pub to: u64,
    pub origin: Influence,
    pub max_zombie_duration: Duration,
}
impl PendingInfluence {
    /// Return current weight and whether this record can be discarded.
    pub fn weight(&self, clock: &dyn Clock) -> (f64, bool) {
        calc_pending_influence(&self.op, self.max_zombie_duration, clock)
    }
}
/// Refresh expiry/timeout before reading status, as Go CheckAndGetStatus does.
/// Use the operator's host clock; sample it after refreshing terminal status. Failed terminal operations
/// contribute zero immediately, but remain pending until the zombie window ends.
pub fn calc_pending_influence(
    op: &Operator,
    max_zombie_duration: Duration,
    clock: &dyn Clock,
) -> (f64, bool) {
    if !op.check_expired() {
        op.check_timeout();
    }
    let status = op.status();
    if !status.is_end() {
        return (1., false);
    }
    let reached = op
        .reach_time_of(status)
        .expect("terminal operator retains reach time");
    let gc = clock
        .now()
        .checked_duration_since(reached)
        .is_some_and(|age| age >= max_zombie_duration);
    (if !gc && status == SUCCESS { 1. } else { 0. }, gc)
}

/// Resume removed IDs and pause added IDs once. Retained IDs keep reference
/// counts. Go logs pause failures; return all typed errors to the host instead.
/// Errors are sorted by ID for deterministic reporting and never abort other IDs.
pub fn pause_and_resume_leader_transfer(
    cluster: &BasicCluster,
    direction: Direction,
    old: &HashSet<u64>,
    new: &HashSet<u64>,
) -> Vec<StoreControlError> {
    for id in old.difference(new) {
        cluster.stores.resume_leader_transfer(*id, direction);
    }
    let mut added: Vec<_> = new.difference(old).copied().collect();
    added.sort_unstable();
    added
        .into_iter()
        .filter_map(|id| cluster.stores.pause_leader_transfer(id, direction).err())
        .collect()
}
