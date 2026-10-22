// Copyright 2026 AsterSQL.

use std::time::{Duration, Instant};

use crate::{Iterator, RollingPolicy, Window, avg, max, min, sum};

/// Fixed bucket count and duration for a rolling counter.
#[derive(Clone, Copy, Debug)]
pub struct RollingCounterOpts {
    pub size: usize,
    pub bucket_duration: Duration,
}

/// Thread safe integer counter with aggregation over unexpired buckets.
/// Samples in the same bucket are summed before aggregation, matching Go.
pub struct RollingCounter<C = fn() -> Instant> {
    policy: RollingPolicy<C>,
}

impl RollingCounter {
    /// Creates a counter using the system monotonic clock.
    pub fn new(opts: RollingCounterOpts) -> Self {
        Self::with_clock(opts, Instant::now as fn() -> Instant)
    }
}

impl<C: Fn() -> Instant + Send + Sync> RollingCounter<C> {
    /// Creates a counter using a caller supplied monotonic clock.
    pub fn with_clock(opts: RollingCounterOpts, clock: C) -> Self {
        Self {
            policy: RollingPolicy::with_clock(Window::new(opts.size), opts.bucket_duration, clock),
        }
    }

    /// Adds an integer after conversion to a floating point bucket sample.
    pub fn add(&self, value: i64) {
        self.policy.add(value as f64);
    }

    /// Reduces unexpired buckets in chronological order under a shared lock.
    /// Returns zero without calling the reducer when the whole window expired.
    /// The callback must not write to this counter while holding the read lock.
    pub fn reduce(&self, f: impl FnOnce(Iterator<'_>) -> f64) -> f64 {
        let state = self
            .policy
            .state
            .read()
            .expect("rolling policy lock poisoned");
        let span = self.policy.timespan(&state);
        let size = state.window.size();
        if span >= size as u128 {
            return 0.0;
        }
        let offset = (state.offset + span as usize + 1) % size;
        f(state.window.iterator(offset, size - span as usize))
    }

    /// Averages bucket samples; an initial empty window yields NaN.
    pub fn avg(&self) -> f64 {
        self.reduce(avg)
    }

    /// Returns the minimum bucket sample, or zero when empty or expired.
    pub fn min(&self) -> f64 {
        self.reduce(min)
    }

    /// Returns the maximum bucket sample, or zero when empty or expired.
    pub fn max(&self) -> f64 {
        self.reduce(max)
    }

    /// Returns the sum of unexpired bucket samples.
    pub fn sum(&self) -> f64 {
        self.reduce(sum)
    }

    /// Converts the floating point sum to an integer, truncating toward zero.
    /// Out of range sums follow the Go oracle's native int64 conversion sentinel.
    pub fn value(&self) -> i64 {
        let value = self.sum();
        if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
            i64::MIN
        } else {
            value as i64
        }
    }

    /// Returns whole elapsed buckets since the last write, without rotating.
    /// A backward full bucket returns the window size, matching Go.
    pub fn timespan(&self) -> u128 {
        let state = self
            .policy
            .state
            .read()
            .expect("rolling policy lock poisoned");
        self.policy.timespan(&state)
    }
}
