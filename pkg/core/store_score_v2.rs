// Copyright 2026 AsterSQL.
//! Region scoring with average available disk space and bounded delta estimates.
use crate::StoreInfo;

impl StoreInfo {
    /// Select v2 only for the exact version string; all other versions use v1.
    pub fn region_score(&self, version: &str, high: f64, low: f64, delta: i64) -> f64 {
        if version == "v2" {
            self.region_score_v2(delta, low)
        } else {
            self.region_score_v1(high, low, delta)
        }
    }

    /// Score disk pressure in GiB, preserving Go's strict delta guards.
    pub fn region_score_v2(&self, delta: i64, low_space_ratio: f64) -> f64 {
        let mut available = self.avg_available() as f64 / 1073741824.0;
        let capacity = self.capacity() as f64 / 1073741824.0;
        let mut used = self.used_size() as f64 / 1073741824.0;
        let diff = capacity - available - used;
        let mut size = self.region_size().wrapping_add(delta) as f64;
        if size < 0.0 {
            size = self.region_size() as f64;
        }
        if self.region_size() != 0 {
            let adjusted = used + used * delta as f64 / self.region_size() as f64;
            if adjusted < 2.0 * used && adjusted > used * 0.5 {
                used = adjusted;
            }
            let adjusted = capacity - used - diff;
            if adjusted > 0.0 && adjusted < capacity {
                available = adjusted;
            }
        }
        // Rust's max suppresses NaN, unlike Go's math.Max.
        let bound = capacity * (1.0 - low_space_ratio);
        let floor = if bound.is_nan() {
            f64::NAN
        } else {
            bound.max(50.0)
        };
        let score = if available >= capacity || capacity < 1.0 {
            size
        } else if available > floor {
            (1.0 + 256.0 * (capacity.ln() - (available - floor + 1.0).ln())
                / (capacity - available + floor - 1.0))
                * size
        } else {
            (1.0 + 256.0 * capacity.ln() / capacity) * size + 1e10 * (floor - available) / floor
        };
        let weight = if self.region_weight().is_nan() {
            f64::NAN
        } else {
            self.region_weight().max(1e-6)
        };
        score / weight
    }
}
