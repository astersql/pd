// Copyright 2026 AsterSQL.
//! Hierarchical location comparison and replica diversity scores.
use crate::{StoreInfo, store_labels::equal_fold};

impl StoreInfo {
    /// Return the first differing location level, or None for the same location.
    /// A missing or empty label matches every value at that level.
    pub fn compare_location(&self, other: &Self, labels: &[&str]) -> Option<usize> {
        labels.iter().position(|key| {
            let left = self.label_value(key);
            let right = other.label_value(key);
            !left.is_empty() && !right.is_empty() && !equal_fold(left, right)
        })
    }
}

/// Sum diversity contributions from stores with a different ID.
/// Each first differing level i contributes 100^(labels.len() - i - 1).
/// Repeated stores contribute repeatedly, matching Go's slice iteration.
pub fn distinct_score(labels: &[&str], stores: &[&StoreInfo], other: &StoreInfo) -> f64 {
    let mut score = 0.0;
    for store in stores {
        if store.id() == other.id() {
            continue;
        }
        if let Some(index) = store.compare_location(other, labels) {
            score += 100_f64.powf((labels.len() - index - 1) as f64);
        }
    }
    score
}
