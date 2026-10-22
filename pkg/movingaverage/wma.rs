// Copyright 2026 AsterSQL.

const DEFAULT_SIZE: usize = 10;

/// A linearly weighted moving average with a fixed window.
#[derive(Debug)]
pub struct Wma {
    records: Vec<f64>,
    next: usize,
    count: usize,
    score: f64,
    sum: f64,
}

impl Wma {
    /// Creates an average with the default ten-point window.
    pub fn new() -> Self {
        Self::with_size(DEFAULT_SIZE as isize)
    }

    /// Creates an average, using the default window when `size` is at most one.
    pub fn with_size(size: isize) -> Self {
        let size = if size > 1 {
            size as usize
        } else {
            DEFAULT_SIZE
        };
        Self {
            records: vec![0.0; size],
            next: 0,
            count: 0,
            score: 0.0,
            sum: 0.0,
        }
    }

    /// Adds a data point, giving the newest point the highest weight.
    pub fn add(&mut self, value: f64) {
        let size = self.records.len();
        self.score = self.score - self.sum + value * size as f64;
        if self.count < size {
            self.sum += value;
            self.count += 1;
        } else {
            self.sum = self.sum - self.records[self.next] + value;
        }
        self.records[self.next] = value;
        self.next = (self.next + 1) % size;
    }

    /// Returns the weighted average, or zero for an empty window.
    pub fn get(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let size = self.records.len();
        let weight = (size + (size - self.count + 1)) * self.count / 2;
        self.score / weight as f64
    }

    /// Clears the window and its instantaneous value.
    pub fn reset(&mut self) {
        self.next = 0;
        self.count = 0;
        self.score = 0.0;
        self.sum = 0.0;
    }

    /// Replaces the window with a single data point.
    pub fn set(&mut self, value: f64) {
        self.reset();
        self.add(value);
    }

    /// Returns the last point added, or zero for an empty window.
    pub fn get_instantaneous(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.records[(self.next + self.records.len() - 1) % self.records.len()]
    }
}

impl Default for Wma {
    fn default() -> Self {
        Self::new()
    }
}
