// Copyright 2026 AsterSQL.

/// A median filter retaining at most the configured number of data points.
#[derive(Clone, Debug)]
pub struct MedianFilter {
    records: Vec<f64>,
    count: u64,
    result: f64,
}

impl MedianFilter {
    /// Creates a filter. Negative sizes panic; zero-sized filters cannot add data.
    pub fn new(size: isize) -> Self {
        assert!(size >= 0, "negative median filter size");
        Self {
            records: vec![0.0; size as usize],
            count: 0,
            result: 0.0,
        }
    }

    /// Adds a point, replacing the oldest point when the window is full.
    pub fn add(&mut self, value: f64) {
        let size = self.records.len() as u64;
        self.records[(self.count % size) as usize] = value;
        self.count = self.count.wrapping_add(1);
        let len = self.count.min(size) as usize;
        let mut work = self.records[..len].to_vec();
        if len == 0 {
            self.result = 0.0;
            return;
        }
        // Match pie.Median's quickselect, including its floating-point comparisons.
        let middle = len / 2;
        let lower = if len % 2 == 0 { middle - 1 } else { middle };
        select_median(&mut work, 0, len, lower, middle + 1);
        self.result = if len % 2 == 0 {
            (work[middle - 1] + work[middle]) / 2.0
        } else {
            work[middle]
        };
    }

    /// Returns the median, or zero after construction or reset.
    pub fn get(&self) -> f64 {
        self.result
    }

    /// Clears the active window, retaining the backing records as in Go.
    pub fn reset(&mut self) {
        self.count = 0;
        self.result = 0.0;
    }

    /// Replaces the active window with a single point.
    pub fn set(&mut self, value: f64) {
        self.records[0] = value;
        self.count = 1;
        self.result = value;
    }

    /// Returns the last point; empty reads retain Go's wrapping-index behavior.
    ///
    /// Panics if the configured window has zero capacity.
    pub fn get_instantaneous(&self) -> f64 {
        self.records[(self.count.wrapping_sub(1) % self.records.len() as u64) as usize]
    }
}

fn select_median(work: &mut [f64], mut start: usize, mut end: usize, lower: usize, upper: usize) {
    while end - start > 1 {
        let pivot_index = (start + end) / 2;
        let pivot = work[pivot_index];
        work.swap(start, pivot_index);
        let mut left = start;
        let mut right = end;
        while left + 1 < right {
            if work[left + 1] < pivot {
                work.swap(left + 1, left);
                left += 1;
            } else {
                work.swap(left + 1, right - 1);
                right -= 1;
            }
        }
        if left > lower {
            end = left;
        } else if left + 1 < upper {
            start = left + 1;
        } else {
            break;
        }
    }
}
