// Copyright 2026 AsterSQL.
//! Owned load values and pending predictions without scheduling decisions.

use crate::{HistoryLoads, Loads};
use astersql_pd_core::StoreInfo;
use astersql_pd_statistics_utils::{REGION_STAT_COUNT, RWType};
use std::collections::HashMap;

/// Region rates in RegionStatKind order, distinct from the four load dimensions.
pub type RegionKindLoads = [f64; REGION_STAT_COUNT];

/// Signed changes in region rates and hot peer count.
/// Loads retain Go's variable-length slice contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Influence {
    pub loads: Vec<f64>,
    pub hot_peer_count: f64,
}

/// Store identity plus caller-owned accumulated influence.
#[derive(Clone, Default)]
pub struct StoreSummaryInfo {
    pub store_info: StoreInfo,
    pub pending_sum: Option<Influence>,
}

/// Snapshot store metadata with no pending influence; duplicate IDs use the last entry.
pub fn summary_store_infos(stores: &[StoreInfo]) -> HashMap<u64, StoreSummaryInfo> {
    stores
        .iter()
        .map(|s| {
            (
                s.id(),
                StoreSummaryInfo {
                    store_info: s.clone(),
                    pending_sum: None,
                },
            )
        })
        .collect()
}

impl StoreSummaryInfo {
    /// None and zero weight leave even an absent sum unchanged.
    /// A later influence longer than the initial vector panics, as in Go.
    pub fn add_influence(&mut self, influence: Option<&Influence>, weight: f64) {
        let Some(influence) = influence.filter(|_| weight != 0.0) else {
            return;
        };
        let sum = self.pending_sum.get_or_insert_with(|| Influence {
            loads: vec![0.0; influence.loads.len()],
            hot_peer_count: 0.0,
        });
        for (i, load) in influence.loads.iter().enumerate() {
            sum.loads[i] += load * weight;
        }
        sum.hot_peer_count += influence.hot_peer_count * weight;
    }
}

/// Four-dimensional load snapshot with independently owned historical samples.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreLoad {
    pub loads: Loads,
    pub hot_peer_count: f64,
    pub history_loads: HistoryLoads,
}

impl StoreLoad {
    /// Preserve current history, but project only the instantaneous rates and count.
    /// Unknown RW values still apply the peer count, without indexing the influence.
    /// A known direction with missing region indexes panics, matching Go.
    pub fn to_load_pred(&self, rw: RWType, influence: Option<&Influence>) -> StoreLoadPred {
        let mut future = StoreLoad {
            loads: self.loads,
            hot_peer_count: self.hot_peer_count,
            ..Default::default()
        };
        if let Some(influence) = influence {
            for (dim, kind) in rw.region_stats().iter().enumerate() {
                future.loads[dim] += influence.loads[kind.as_i32() as usize];
            }
            future.hot_peer_count += influence.hot_peer_count;
        }
        StoreLoadPred {
            current: self.clone(),
            future,
            ..Default::default()
        }
    }
}

/// Current and predicted rates, with expectation and standard deviation for consumers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreLoadPred {
    pub current: StoreLoad,
    pub future: StoreLoad,
    pub expect: StoreLoad,
    pub stddev: StoreLoad,
}

impl StoreLoadPred {
    pub fn min(&self) -> StoreLoad {
        min_load(&self.current, &self.future)
    }
    pub fn max(&self) -> StoreLoad {
        max_load(&self.current, &self.future)
    }
    /// Absolute rate changes; deliberately excludes peer-count influence.
    pub fn pending(&self) -> StoreLoad {
        StoreLoad {
            hot_peer_count: 0.0,
            ..self.diff()
        }
    }
    /// Per-dimension maximum minus minimum, including the independent peer count.
    pub fn diff(&self) -> StoreLoad {
        let max = self.max();
        let min = self.min();
        StoreLoad {
            loads: std::array::from_fn(|i| max.loads[i] - min.loads[i]),
            hot_peer_count: max.hot_peer_count - min.hot_peer_count,
            ..Default::default()
        }
    }
}

/// Per-dimension minimum, with empty history and an independently minimized peer count.
pub fn min_load(a: &StoreLoad, b: &StoreLoad) -> StoreLoad {
    StoreLoad {
        loads: std::array::from_fn(|i| go_min(a.loads[i], b.loads[i])),
        hot_peer_count: go_min(a.hot_peer_count, b.hot_peer_count),
        ..Default::default()
    }
}

/// Per-dimension maximum, with empty history and an independently maximized peer count.
pub fn max_load(a: &StoreLoad, b: &StoreLoad) -> StoreLoad {
    StoreLoad {
        loads: std::array::from_fn(|i| go_max(a.loads[i], b.loads[i])),
        hot_peer_count: go_max(a.hot_peer_count, b.hot_peer_count),
        ..Default::default()
    }
}

// Go math.Min/Max prioritize their respective infinity over NaN and preserve
// signed zero. Rust f64::min/max instead ignore a single NaN operand.
fn go_min(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY || b == f64::NEG_INFINITY {
        f64::NEG_INFINITY
    } else if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 && (a.is_sign_negative() || b.is_sign_negative()) {
        -0.0
    } else {
        a.min(b)
    }
}
fn go_max(a: f64, b: f64) -> f64 {
    if a == f64::INFINITY || b == f64::INFINITY {
        f64::INFINITY
    } else if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 && (a.is_sign_positive() || b.is_sign_positive()) {
        0.0
    } else {
        a.max(b)
    }
}
