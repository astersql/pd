// Copyright 2026 AsterSQL.

use crate::{AvgOverTime, MedianFilter};

/// Smooths trailing change rates with a median filter.
/// The minimum filled period is the average size times the report interval.
#[derive(Clone, Debug)]
pub struct TimeMedian {
    average: AvgOverTime,
    median: MedianFilter,
}

impl TimeMedian {
    /// Creates a filter with signed nanosecond intervals, matching Go durations.
    /// The median size must be positive when samples are added.
    pub fn new(average_size: isize, median_size: isize, report_interval: i64) -> Self {
        Self {
            average: AvgOverTime::new((average_size as i64).wrapping_mul(report_interval)),
            median: MedianFilter::new(median_size),
        }
    }

    /// Returns the median of the retained full-window rates.
    pub fn get(&self) -> f64 {
        self.median.get()
    }

    /// Adds a change and interval, sampling the average whenever it is full.
    pub fn add(&mut self, delta: f64, interval: i64) {
        self.average.add(delta, interval);
        if self.average.is_full() {
            self.median.add(self.average.get());
        }
    }

    /// Replaces the median history while preserving the time average's samples.
    pub fn set(&mut self, average: f64) {
        self.median.set(average);
    }

    /// Returns the most recent change rate, including before the window is full.
    pub fn get_instantaneous(&self) -> f64 {
        self.average.get_instantaneous()
    }
}
