// Copyright 2026 AsterSQL.
//! Available-space smoothing using the same 60-sample Hull average as Go.

/// Clamp nonpositive values to zero and truncate positive byte counts.
/// Inputs from heartbeat smoothing are finite. For out-of-range floats Rust's
/// saturating conversion applies; Go does not specify a portable result there.
pub fn climp0(value: f64) -> u64 {
    if value <= 0.0 { 0 } else { value as u64 }
}

impl super::StoreInfo {
    /// Smoothed available bytes, or raw bytes for a zero-value store without HMA.
    /// Capacity does not affect this value, including zero capacity.
    pub fn avg_available(&self) -> u64 {
        self.avg_available
            .as_ref()
            .map_or_else(|| self.available(), |average| climp0(average.get()))
    }
}
