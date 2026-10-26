// Copyright 2026 AsterSQL.
//! Slow-trend detection over canonical core store snapshots. Clocks are supplied
//! as Unix nanoseconds by the caller; persistence and leader eviction are separate.
use astersql_pd_core::{BasicCluster, StoreInfo};
use std::sync::Arc;

const TREND_EPSILON: f64 = 1e-9;
const RECHECK_SECONDS: u64 = 120;

/// Captured candidate and the two independently observed clock readings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlowTrendCandidate {
    pub store_id: u64,
    pub capture_ts: i64,
    pub recover_ts: i64,
}
impl SlowTrendCandidate {
    /// Whole elapsed seconds; the host supplies a nondecreasing clock.
    pub fn captured_secs(&self, now: i64) -> u64 {
        elapsed_secs(self.capture_ts, now)
    }
}
fn elapsed_secs(start: i64, now: i64) -> u64 {
    ((i128::from(now) - i128::from(start)).max(0) / 1_000_000_000) as u64
}

/// In-memory candidate history; this does not mark a store evicted in core.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlowTrendCandidates {
    pub current: Option<SlowTrendCandidate>,
    pub last: Option<SlowTrendCandidate>,
}
impl SlowTrendCandidates {
    /// Capture initializes history only once, matching captureCandidate.
    pub fn capture(&mut self, store_id: u64, capture_ts: i64, recover_ts: i64) {
        self.current = Some(SlowTrendCandidate {
            store_id,
            capture_ts,
            recover_ts,
        });
        if self.last.is_none() {
            self.last = self.current;
        }
    }
    /// Clear the current candidate and optionally replace history, even if empty.
    pub fn pop(&mut self, update_last: bool) -> u64 {
        if update_last {
            self.last = self.current;
        }
        self.current.take().map_or(0, |c| c.store_id)
    }
    /// Mark recovery only when history exists.
    pub fn mark_recovered(&mut self, now: i64) {
        if let Some(last) = &mut self.last {
            last.recover_ts = now;
        }
    }
}
fn eligible(s: &StoreInfo) -> bool {
    !s.is_removed() && (s.is_preparing() || s.is_serving())
}

/// Choose the unique slow store. Ratio and engine mode come from scheduler
/// configuration; all stores, including removed stores, contribute to denominators.
pub fn choose_slow_trend_candidate(
    cluster: &BasicCluster,
    raft_v2: bool,
    affected_ratio: f64,
    last: Option<&SlowTrendCandidate>,
    now: i64,
) -> Option<Arc<StoreInfo>> {
    let stores = cluster.stores.get_stores();
    if stores.len() < 3 {
        return None;
    }
    let mut candidate = None;
    let mut affected = 0;
    for s in &stores {
        if !eligible(s) {
            continue;
        }
        let Some(t) = s.store_stats().slow_trend else {
            continue;
        };
        if t.result_rate < -TREND_EPSILON {
            affected += 1;
        }
        if t.cause_rate > TREND_EPSILON
            && (t.result_rate < -TREND_EPSILON
                || (raft_v2
                    && last.is_some_and(|l| {
                        l.store_id == s.id() && elapsed_secs(l.recover_ts, now) <= RECHECK_SECONDS
                    })))
        {
            if candidate.is_some() {
                return None;
            }
            candidate = Some(Arc::clone(s));
        }
    }
    let candidate = candidate?;
    let threshold = (stores.len() as f64 * affected_ratio) as i64;
    if (affected as i64) < threshold || !slower_than(&stores, &candidate) {
        return None;
    }
    Some(candidate)
}

/// Require ceil(n/2) updated stores. The candidate and ineligible stores count
/// immediately; another eligible store must have heartbeat >= capture timestamp.
pub fn slow_trend_stores_updated(cluster: &BasicCluster, slow_id: u64, captured_at: i64) -> bool {
    let stores = cluster.stores.get_stores();
    stores.len() > 1
        && stores
            .iter()
            .filter(|s| {
                !eligible(s) || s.id() == slow_id || s.last_heartbeat_unix_nanos() >= captured_at
            })
            .count()
            >= stores.len().div_ceil(2)
}

fn slower_than(stores: &[Arc<StoreInfo>], target: &StoreInfo) -> bool {
    let Some(t) = target.store_stats().slow_trend else {
        return false;
    };
    stores
        .iter()
        .filter(|s| {
            eligible(s)
                && s.id() != target.id()
                && s.store_stats().slow_trend.is_some_and(|other| {
                    t.cause_value - other.cause_value > TREND_EPSILON
                        && other.cause_value > TREND_EPSILON
                })
        })
        .count()
        >= (stores.len() * 2 + 1) / 3
}
/// Strict cause-value comparison against floor((2*n+1)/3) peers.
pub fn slow_trend_slower_than_others(cluster: &BasicCluster, target: &StoreInfo) -> bool {
    slower_than(&cluster.stores.get_stores(), target)
}
/// Recovery/cancellation comparison against ceil(n/2) peers, including the
/// source's inclusive 10% tolerance and positive-data guards.
pub fn slow_trend_faster_than_others(cluster: &BasicCluster, target: &StoreInfo) -> bool {
    let stores = cluster.stores.get_stores();
    let Some(t) = target.store_stats().slow_trend else {
        return false;
    };
    stores
        .iter()
        .filter(|s| {
            eligible(s)
                && s.id() != target.id()
                && s.store_stats().slow_trend.is_some_and(|other| {
                    t.cause_value <= other.cause_value * 1.1
                        && other.cause_value > TREND_EPSILON
                        && t.cause_value > TREND_EPSILON
                })
        })
        .count()
        >= stores.len().div_ceil(2)
}
