// Copyright 2026 AsterSQL.

use crate::Wma;

/// A Hull moving average composed of half, full, and square-root weighted windows.
#[derive(Debug)]
pub struct Hma {
    half: Wma,
    full: Wma,
    smoothed: Wma,
}

impl Hma {
    /// Creates an average with the default ten-point window.
    pub fn new() -> Self {
        Self::with_size(10.0)
    }

    /// Creates an average, truncating `size` when greater than one.
    ///
    /// Other values use the default size of ten. Derived windows of at most one
    /// retain Wma's default ten-point window, matching the Go implementation.
    pub fn with_size(size: f64) -> Self {
        let size = if size > 1.0 { size as isize } else { 10 };
        Self {
            half: Wma::with_size(size / 2),
            full: Wma::with_size(size),
            smoothed: Wma::with_size((size as f64).sqrt() as isize),
        }
    }

    /// Adds a data point and smooths twice the half-window minus the full window.
    pub fn add(&mut self, value: f64) {
        self.half.add(value);
        self.full.add(value);
        self.smoothed.add(2.0 * self.half.get() - self.full.get());
    }

    /// Returns the Hull average, or zero when empty.
    pub fn get(&self) -> f64 {
        self.smoothed.get()
    }

    /// Clears all three windows.
    pub fn reset(&mut self) {
        self.half.reset();
        self.full.reset();
        self.smoothed.reset();
    }

    /// Replaces all history with a single data point.
    pub fn set(&mut self, value: f64) {
        self.reset();
        self.add(value);
    }

    /// Returns the last input point, or zero when empty.
    pub fn get_instantaneous(&self) -> f64 {
        self.full.get_instantaneous()
    }
}

impl Default for Hma {
    fn default() -> Self {
        Self::new()
    }
}
