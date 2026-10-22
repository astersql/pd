// Copyright 2026 AsterSQL.

//! Bucket mutations and circular window storage, matching `window.go`.

#![forbid(unsafe_code)]

/// Floating point samples and the number of append/add operations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bucket {
    pub points: Vec<f64>,
    pub count: i64,
}

impl Bucket {
    /// Appends a sample and records one operation.
    pub fn append(&mut self, value: f64) {
        self.points.push(value);
        self.count = self.count.wrapping_add(1);
    }

    /// Adds to an existing sample, panicking if the point offset is invalid.
    pub fn add(&mut self, offset: usize, value: f64) {
        self.points[offset] += value;
        self.count = self.count.wrapping_add(1);
    }

    /// Clears samples and the operation count while retaining allocation.
    pub fn reset(&mut self) {
        self.points.clear();
        self.count = 0;
    }
}

/// Fixed size circular collection of buckets.
///
/// Mutation requires exclusive access; shared callers supply their own lock.
/// Offsets are nonnegative and wrap modulo the size. Accessing a bucket in a
/// zero sized window panics, matching Go; resetting the whole empty window is safe.
#[derive(Clone, Debug)]
pub struct Window {
    buckets: Vec<Bucket>,
}

impl Window {
    /// Creates a window with `size` empty buckets.
    pub fn new(size: usize) -> Self {
        Self {
            buckets: vec![Bucket::default(); size],
        }
    }

    /// Returns the fixed number of buckets.
    pub fn size(&self) -> usize {
        self.buckets.len()
    }

    /// Borrows the bucket at the circular offset.
    pub fn bucket(&self, offset: usize) -> &Bucket {
        &self.buckets[offset % self.size()]
    }

    fn bucket_mut(&mut self, offset: usize) -> &mut Bucket {
        let index = offset % self.size();
        &mut self.buckets[index]
    }

    /// Appends a sample to the bucket at the circular offset.
    pub fn append(&mut self, offset: usize, value: f64) {
        self.bucket_mut(offset).append(value);
    }

    /// Adds to the first sample, or appends when the bucket count is zero.
    ///
    /// This follows the Go implementation, including its choice of point zero
    /// when a bucket contains more than one sample.
    pub fn add(&mut self, offset: usize, value: f64) {
        let bucket = self.bucket_mut(offset);
        if bucket.count == 0 {
            bucket.append(value);
        } else {
            bucket.add(0, value);
        }
    }

    /// Clears the bucket at the circular offset.
    pub fn reset_bucket(&mut self, offset: usize) {
        self.bucket_mut(offset).reset();
    }

    /// Clears `count` consecutive buckets, wrapping around the window.
    pub fn reset_buckets(&mut self, offset: usize, count: usize) {
        for i in 0..count {
            self.reset_bucket(offset.wrapping_add(i));
        }
    }

    /// Clears every bucket while retaining their allocations.
    pub fn reset_window(&mut self) {
        for bucket in &mut self.buckets {
            bucket.reset();
        }
    }
}

#[cfg(test)]
mod window_test;

#[cfg(test)]
mod reduce_test;

mod reduce;
pub use reduce::{Iterator, avg, count, max, min, sum};
