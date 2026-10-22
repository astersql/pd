// Copyright 2026 AsterSQL.

use std::collections::VecDeque;

#[derive(Clone, Debug, Default)]
struct DeltaWithInterval {
    delta: f64,
    interval: i64,
}

/// Tracks the change rate over a trailing window, returning zero until full.
/// Intervals use signed nanoseconds, matching Go's time.Duration.
#[derive(Clone, Debug)]
pub struct AvgOverTime {
    queue: VecDeque<DeltaWithInterval>,
    margin: DeltaWithInterval,
    delta_sum: f64,
    interval_sum: i64,
    avg_interval: i64,
}

impl AvgOverTime {
    /// Creates an average with the given window in nanoseconds.
    pub fn new(interval: i64) -> Self {
        Self {
            queue: VecDeque::new(),
            margin: DeltaWithInterval::default(),
            delta_sum: 0.0,
            interval_sum: 0,
            avg_interval: interval,
        }
    }

    /// Adds a change and its interval in nanoseconds. Zero intervals are ignored.
    pub fn add(&mut self, delta: f64, interval: i64) {
        if interval == 0 {
            return;
        }
        self.queue.push_back(DeltaWithInterval { delta, interval });
        self.delta_sum += delta;
        self.interval_sum = self.interval_sum.wrapping_add(interval);
        // Keep the oldest overlapping sample as the margin, trimming it only
        // when reading so fractional-window contributions retain Go's rounding.
        while self.interval_sum.wrapping_sub(self.margin.interval) >= self.avg_interval {
            self.delta_sum -= self.margin.delta;
            self.interval_sum = self.interval_sum.wrapping_sub(self.margin.interval);
            self.margin = self.queue.pop_front().expect("empty average queue");
        }
    }

    /// Returns the change per second within the window, or zero before full.
    pub fn get(&self) -> f64 {
        if !self.is_full() {
            return 0.0;
        }
        let margin_delta = self.margin.delta
            * (seconds(self.interval_sum) - seconds(self.avg_interval))
            / seconds(self.margin.interval);
        (self.delta_sum - margin_delta) / seconds(self.avg_interval)
    }

    /// Clears samples while retaining the configured window.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.margin = DeltaWithInterval::default();
        self.delta_sum = 0.0;
        self.interval_sum = 0;
    }

    /// Seeds the average with a full window at the given rate.
    pub fn set(&mut self, avg: f64) {
        self.clear();
        self.margin = DeltaWithInterval {
            delta: avg * seconds(self.avg_interval),
            interval: self.avg_interval,
        };
        self.delta_sum = self.margin.delta;
        self.interval_sum = self.avg_interval;
        self.queue.push_back(self.margin.clone());
    }

    /// Returns the retained interval sum in signed nanoseconds.
    pub fn get_interval_sum(&self) -> i64 {
        self.interval_sum
    }

    /// Returns the latest sample's rate, even before the window is full.
    pub fn get_instantaneous(&self) -> f64 {
        if let Some(data) = self.queue.back() {
            return data.delta / seconds(data.interval);
        }
        if self.margin.interval != 0 {
            return self.margin.delta / seconds(self.margin.interval);
        }
        self.margin.delta
    }

    /// Returns whether samples cover the entire configured window.
    pub fn is_full(&self) -> bool {
        self.interval_sum >= self.avg_interval
    }
}

fn seconds(nanos: i64) -> f64 {
    // Match time.Duration.Seconds rather than converting the entire i64 at once.
    (nanos / 1_000_000_000) as f64 + (nanos % 1_000_000_000) as f64 / 1_000_000_000.0
}
