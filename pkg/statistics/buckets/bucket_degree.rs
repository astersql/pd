// Copyright 2026 AsterSQL.

use std::collections::HashMap;

use astersql_pd_statistics_utils::MIN_HOT_THRESHOLDS;

use crate::{BucketRangeIndex, BucketStat, BucketTreeItem};

/// Lowest degree reached by repeated cold reports.
pub const MIN_HOT_DEGREE: i32 = -20;
/// Highest degree reached by repeated hot reports.
pub const MAX_HOT_DEGREE: i32 = 20;

impl BucketTreeItem {
    /// Advances each bucket's heat by one, using read/write byte and key rates.
    ///
    /// A rate must strictly exceed its threshold. Query rates are ignored.
    /// Degrees already beyond a bound retain Go's conditional-step behavior.
    pub fn calculate_hot_degree(&mut self) {
        for stat in &mut self.stats {
            let hot = [0, 1, 3, 4]
                .into_iter()
                .any(|dim| stat.loads[dim] > MIN_HOT_THRESHOLDS[dim] as u64);
            if hot && stat.hot_degree < MAX_HOT_DEGREE {
                stat.hot_degree += 1;
            }
            if !hot && stat.hot_degree > MIN_HOT_DEGREE {
                stat.hot_degree -= 1;
            }
        }
    }
}

impl BucketRangeIndex {
    /// Returns owned current-region stats whose degree is at least `min_degree`.
    ///
    /// An empty ID list selects all current regions. Unknown IDs and regions
    /// without matching stats are omitted; duplicate IDs do not duplicate stats.
    /// Archived range fragments are excluded. Each region retains bucket order.
    pub fn get_hot_bucket_stats(
        &self,
        min_degree: i32,
        region_ids: &[u64],
    ) -> HashMap<u64, Vec<BucketStat>> {
        let mut result = HashMap::new();
        let mut append = |item: &BucketTreeItem| {
            let stats: Vec<_> = item
                .stats
                .iter()
                .filter(|stat| stat.hot_degree >= min_degree)
                .cloned()
                .collect();
            if !stats.is_empty() {
                result.insert(item.region_id, stats);
            }
        };
        if region_ids.is_empty() {
            for item in self.current_regions() {
                append(item);
            }
        } else {
            for &id in region_ids {
                if let Some(item) = self.region(id) {
                    append(item);
                }
            }
        }
        result
    }
}
