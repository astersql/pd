// Copyright 2026 AsterSQL.

/// A maximum filter retaining at most the configured number of data points.
#[derive(Clone, Debug)]
pub struct MaxFilter {
    records: Vec<f64>,
    count: u64,
}

impl MaxFilter {
    /// Creates a filter. Negative sizes panic; zero capacity permits empty reads.
    pub fn new(size: isize) -> Self {
        assert!(size >= 0, "negative max filter size");
        Self {
            records: vec![0.0; size as usize],
            count: 0,
        }
    }

    /// Adds a point, replacing the oldest point when full. Zero capacity panics.
    pub fn add(&mut self, value: f64) {
        let index = (self.count % self.records.len() as u64) as usize;
        self.records[index] = value;
        self.count = self.count.wrapping_add(1);
    }

    /// Returns the maximum, or zero when empty, with Go's ordered comparisons.
    pub fn get(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let len = self.count.min(self.records.len() as u64) as usize;
        let mut maximum = self.records[0];
        // f64::max would change pie.Max's NaN and signed-zero behavior.
        for &value in &self.records[..len] {
            if value > maximum {
                maximum = value;
            }
        }
        maximum
    }

    /// Clears the active window while retaining allocated storage.
    pub fn reset(&mut self) {
        self.count = 0;
    }

    /// Replaces the active window with a single point. Zero capacity panics.
    pub fn set(&mut self, value: f64) {
        self.records[0] = value;
        self.count = 1;
    }

    /// Returns the latest point, or zero when empty.
    pub fn get_instantaneous(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.records[((self.count - 1) % self.records.len() as u64) as usize]
    }
}
