// Copyright 2026 AsterSQL.

use astersql_pd_movingaverage::TimeMedian;
use astersql_pd_statistics_utils::{ActionType, DEFAULT_AOT_SIZE, DIM_LEN, RWType, TopNItem};

const SECOND: i64 = 1_000_000_000;
const ROLLING_WINDOWS_SIZE: isize = 5;

/// One load dimension's median and average since the last accounting boundary.
/// Mutation requires exclusive access; a sharing host supplies synchronization.
#[derive(Debug)]
pub struct DimStat {
    rolling: TimeMedian,
    last_interval_sum: i64,
    last_delta: f64,
}

/// Matches Go dimStat.clone: copy the window and seconds, reset accumulated delta.
impl Clone for DimStat {
    fn clone(&self) -> Self {
        Self {
            rolling: self.rolling.clone(),
            last_interval_sum: self.last_interval_sum,
            last_delta: 0.0,
        }
    }
}

impl DimStat {
    // Direct reports preserve pending delta, unlike Go inheritance clone semantics.
    pub(crate) fn copy_for_report(&self) -> Self {
        Self {
            rolling: self.rolling.clone(),
            last_interval_sum: self.last_interval_sum,
            last_delta: self.last_delta,
        }
    }

    /// Creates a real TimeMedian; report interval uses signed nanoseconds.
    pub fn new(report_interval: i64) -> Self {
        Self {
            rolling: TimeMedian::new(
                DEFAULT_AOT_SIZE as isize,
                ROLLING_WINDOWS_SIZE,
                report_interval,
            ),
            last_interval_sum: 0,
            last_delta: 0.0,
        }
    }

    /// Adds a cumulative delta. Each interval truncates separately to whole
    /// seconds for last-average accounting; the rolling filter keeps nanoseconds.
    pub fn add(&mut self, delta: f64, interval: i64) {
        self.last_interval_sum = self
            .last_interval_sum
            .wrapping_add(seconds(interval) as i64);
        self.last_delta += delta;
        self.rolling.add(delta, interval);
    }

    /// Returns the median of full-window rates, zero before the first full window.
    pub fn get(&self) -> f64 {
        self.rolling.get()
    }

    /// Tests the median against an inclusive threshold.
    pub fn is_hot(&self, threshold: f64) -> bool {
        self.get() >= threshold
    }

    /// Tests the accumulated last average, including IEEE zero-division behavior.
    pub fn is_last_average_hot(&self, threshold: f64) -> bool {
        self.last_delta / self.last_interval_sum as f64 >= threshold
    }

    /// Tests whole accumulated seconds against a truncated reporting interval.
    pub fn is_full(&self, interval: i64) -> bool {
        self.last_interval_sum >= seconds(interval) as i64
    }

    /// Resets accounting without discarding the rolling median's history.
    pub fn clear_last_average(&mut self) {
        self.last_interval_sum = 0;
        self.last_delta = 0.0;
    }
}

/// One peer's directional byte/key/query/CPU loads and hotspot metadata.
/// `None` rolling loads preserve instantaneous snapshot semantics; present but
/// missing dimensions fall back to instantaneous loads. Host code owns time.
#[derive(Debug, Default)]
pub struct HotPeerStat {
    pub store_id: u64,
    pub region_id: u64,
    pub hot_degree: i64,
    pub anti_count: i64,
    pub loads: Vec<f64>,
    pub rolling_loads: Option<Vec<Option<DimStat>>>,
    pub stores: Vec<u64>,
    pub action_type: ActionType,
    pub is_leader: bool,
    /// Host-clock nanoseconds; None represents Go's unset/zero transfer time.
    pub last_transfer_leader_time: Option<i64>,
    pub in_cold: bool,
    pub allow_inherited: bool,
}

/// Produces Go HotPeerStat.Clone's four-dimensional denoised snapshot.
/// The snapshot has no rolling state and owns its loads and store list.
impl Clone for HotPeerStat {
    fn clone(&self) -> Self {
        Self {
            store_id: self.store_id,
            region_id: self.region_id,
            hot_degree: self.hot_degree,
            anti_count: self.anti_count,
            loads: (0..DIM_LEN)
                .map(|dim| self.get_load(dim as isize))
                .collect(),
            rolling_loads: None,
            stores: self.stores.clone(),
            action_type: self.action_type,
            is_leader: self.is_leader,
            last_transfer_leader_time: self.last_transfer_leader_time,
            in_cold: self.in_cold,
            allow_inherited: self.allow_inherited,
        }
    }
}

impl HotPeerStat {
    /// Returns a rounded denoised load, falling back to instantaneous load.
    /// Invalid dimensions and absent values return zero, including short vectors.
    pub fn get_load(&self, dim: isize) -> f64 {
        if dim < 0 || dim as usize >= DIM_LEN {
            return 0.0;
        }
        let dim = dim as usize;
        if let Some(Some(rolling)) = self.rolling_loads.as_ref().and_then(|loads| loads.get(dim)) {
            return rolling.get().round();
        }
        self.loads.get(dim).copied().unwrap_or(0.0).round()
    }

    /// Returns an owned view. Without rolling state preserve raw length/values;
    /// with rolling state return exactly four rounded, independently read loads.
    pub fn get_loads(&self) -> Vec<f64> {
        if self.rolling_loads.is_some() {
            (0..DIM_LEN)
                .map(|dim| self.get_load(dim as isize))
                .collect()
        } else {
            self.loads.clone()
        }
    }

    /// Any present rolling dimension can be hot by its last average, including CPU.
    /// Panics if thresholds omit a present dimension that must be examined, as Go does.
    pub fn is_hot(&self, thresholds: &[f64]) -> bool {
        self.rolling_loads.as_ref().is_some_and(|loads| {
            loads.iter().enumerate().any(|(dim, load)| {
                load.as_ref()
                    .is_some_and(|load| load.is_last_average_hot(thresholds[dim]))
            })
        })
    }

    /// Clears only last averages, skipping absent dimensions.
    pub fn clear_last_average(&mut self) {
        if let Some(loads) = &mut self.rolling_loads {
            for load in loads.iter_mut().flatten() {
                load.clear_last_average();
            }
        }
    }

    /// Returns the first dimension's accumulated interval in nanoseconds, or zero.
    pub fn interval_sum(&self) -> i64 {
        self.rolling_loads
            .as_ref()
            .and_then(|loads| loads.first())
            .and_then(Option::as_ref)
            .map_or(0, |load| load.last_interval_sum.wrapping_mul(SECOND))
    }

    /// Uses explicit host-clock nanoseconds and a strict cooldown boundary.
    /// A future transfer time remains cooling down; an unset time does not.
    pub fn needs_cooldown_transfer_leader(
        &self,
        now: i64,
        min_hot_degree: i64,
        rw: RWType,
    ) -> bool {
        self.last_transfer_leader_time.is_some_and(|last| {
            let elapsed = now as i128 - last as i128;
            let elapsed_seconds = (elapsed / SECOND as i128) as f64
                + (elapsed % SECOND as i128) as f64 / SECOND as f64;
            elapsed_seconds < min_hot_degree.wrapping_mul(rw.report_interval() as i64) as f64
        })
    }
}

impl TopNItem for HotPeerStat {
    fn id(&self) -> u64 {
        self.region_id
    }

    fn less(&self, dim: usize, than: &Self) -> bool {
        self.get_load(dim as isize) < than.get_load(dim as isize)
    }
}

fn seconds(nanos: i64) -> f64 {
    (nanos / SECOND) as f64 + (nanos % SECOND) as f64 / SECOND as f64
}
