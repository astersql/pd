// Copyright 2026 AsterSQL.
//! Original region scoring across high, transition and low space stages.
use crate::StoreInfo;

impl StoreInfo {
    /// Score raw space in MiB, applying compression amplification and region weight.
    /// Signed size addition wraps like Go; NaN weights propagate through math.Max.
    pub fn region_score_v1(&self, high_space_ratio: f64, low_space_ratio: f64, delta: i64) -> f64 {
        let available = self.available() as f64 / 1048576.0;
        let used = self.used_size() as f64 / 1048576.0;
        let capacity = self.capacity() as f64 / 1048576.0;
        let amplification = if self.region_size() == 0 || used == 0.0 {
            1.0
        } else {
            self.region_size() as f64 / used
        };
        let high = (1.0 - high_space_ratio) * capacity;
        let low = (1.0 - low_space_ratio) * capacity;
        let adjusted = available - delta as f64 / amplification;
        let size = self.region_size().wrapping_add(delta) as f64;
        let score = if adjusted >= high {
            size
        } else if adjusted <= low {
            1073741824.0 - adjusted
        } else {
            let x1 = (used + available - high) * amplification;
            let x2 = (used + available - low) * amplification;
            let y2 = 1073741824.0 - low;
            let k = (y2 - x1) / (x2 - x1);
            let b = x1 - k * x1;
            k * size + b
        };
        let weight = if self.region_weight().is_nan() {
            f64::NAN
        } else {
            self.region_weight().max(1e-6)
        };
        score / weight
    }
}
