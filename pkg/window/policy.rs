// Copyright 2026 AsterSQL.

use std::{
    sync::RwLock,
    time::{Duration, Instant},
};

use crate::Window;

/// A time based ring policy. A single lock protects rotation and bucket writes.
///
/// Uses the same standard blocking lock as syncutil. Clocks must be thread safe;
/// the default clock uses monotonic `Instant` rather than wall time.
pub struct RollingPolicy<C = fn() -> Instant> {
    pub(super) state: RwLock<PolicyState>,
    bucket_duration: Duration,
    clock: C,
}

pub(super) struct PolicyState {
    pub(super) window: Window,
    pub(super) offset: usize,
    pub(super) last_append_time: Instant,
}

impl RollingPolicy {
    /// Creates a policy using the system monotonic clock.
    pub fn new(window: Window, bucket_duration: Duration) -> Self {
        Self::with_clock(window, bucket_duration, Instant::now as fn() -> Instant)
    }
}

impl<C: Fn() -> Instant + Send + Sync> RollingPolicy<C> {
    /// Creates a policy with an injectable clock, sampled at construction and add.
    /// A zero bucket duration panics on add, matching Go division by zero.
    pub fn with_clock(window: Window, bucket_duration: Duration, clock: C) -> Self {
        let last_append_time = clock();
        Self {
            state: RwLock::new(PolicyState {
                window,
                offset: 0,
                last_append_time,
            }),
            bucket_duration,
            clock,
        }
    }

    /// Returns elapsed whole buckets. A backward full bucket expires the window;
    /// a backward fractional bucket truncates to zero, matching Go.
    pub(super) fn timespan(&self, state: &PolicyState) -> u128 {
        let now = (self.clock)();
        if now >= state.last_append_time {
            now.duration_since(state.last_append_time).as_nanos() / self.bucket_duration.as_nanos()
        } else if state.last_append_time.duration_since(now).as_nanos()
            / self.bucket_duration.as_nanos()
            > 0
        {
            state.window.size() as u128
        } else {
            0
        }
    }

    /// Adds to the current bucket, clearing expired buckets before writing.
    pub fn add(&self, value: f64) {
        let mut state = self.state.write().expect("rolling policy lock poisoned");
        let span = self.timespan(&state);
        if span > 0 {
            let size = state.window.size();
            let start = (state.offset + 1) % size;
            let end = (state.offset + (span % size as u128) as usize) % size;
            state
                .window
                .reset_buckets(start, span.min(size as u128) as usize);
            state.offset = end;
            // Advance by whole buckets, preserving fractional elapsed time.
            let nanos = span * self.bucket_duration.as_nanos();
            let elapsed = Duration::new(
                u64::try_from(nanos / 1_000_000_000).expect("rolling duration overflow"),
                (nanos % 1_000_000_000) as u32,
            );
            state.last_append_time += elapsed;
        }
        let offset = state.offset;
        state.window.add(offset, value);
    }
}
