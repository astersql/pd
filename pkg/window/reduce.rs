// Copyright 2026 AsterSQL.

use crate::{Bucket, Window};

/// A bounded circular bucket cursor. Availability checks do not advance it.
#[derive(Clone, Debug)]
pub struct Iterator<'a> {
    window: &'a Window,
    offset: usize,
    remaining: usize,
}

impl Window {
    /// Visits `count` buckets starting at the circular `offset`, including repeats.
    /// Panics for a zero sized window, even when `count` is zero, matching Go.
    pub fn iterator(&self, offset: usize, count: usize) -> Iterator<'_> {
        Iterator {
            window: self,
            offset: offset % self.size(),
            remaining: count,
        }
    }
}

impl<'a> Iterator<'a> {
    /// Reports whether a bucket remains without advancing the cursor.
    pub fn has_next(&self) -> bool {
        self.remaining != 0
    }

    /// Returns the current bucket and advances, panicking after exhaustion.
    pub fn bucket(&mut self) -> &'a Bucket {
        assert!(self.has_next(), "stat/metric: iteration out of range");
        let bucket = self.window.bucket(self.offset);
        self.remaining -= 1;
        self.offset = if self.offset == self.window.size() - 1 {
            0
        } else {
            self.offset + 1
        };
        bucket
    }
}

/// Sums sample values in traversal order.
pub fn sum(mut iterator: Iterator<'_>) -> f64 {
    let mut result = 0.0;
    while iterator.has_next() {
        for point in &iterator.bucket().points {
            result += point;
        }
    }
    result
}

/// Averages samples (rather than operations), returning NaN for no samples.
pub fn avg(mut iterator: Iterator<'_>) -> f64 {
    let (mut result, mut count) = (0.0, 0.0);
    while iterator.has_next() {
        for point in &iterator.bucket().points {
            result += point;
            count += 1.0;
        }
    }
    result / count
}

fn extreme(mut iterator: Iterator<'_>, compare: impl Fn(f64, f64) -> bool) -> f64 {
    let mut result = None;
    while iterator.has_next() {
        for &point in &iterator.bucket().points {
            if result.is_none_or(|current| compare(point, current)) {
                result = Some(point);
            }
        }
    }
    result.unwrap_or(0.0)
}

/// Returns the minimum sample, or zero for no samples, preserving Go NaN comparisons.
pub fn min(iterator: Iterator<'_>) -> f64 {
    extreme(iterator, |a, b| a < b)
}

/// Returns the maximum sample, or zero for no samples, preserving Go NaN comparisons.
pub fn max(iterator: Iterator<'_>) -> f64 {
    extreme(iterator, |a, b| a > b)
}

/// Sums operation counts with Go's wrapping int64 arithmetic before conversion.
pub fn count(mut iterator: Iterator<'_>) -> f64 {
    let mut result = 0_i64;
    while iterator.has_next() {
        result = result.wrapping_add(iterator.bucket().count);
    }
    result as f64
}
