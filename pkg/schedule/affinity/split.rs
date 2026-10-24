// Copyright 2026 AsterSQL.
use crate::manager::Manager;
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_labeler::RegionLabeler;
use std::sync::Arc;
/// Domain split cause; protocol conversion belongs to the host adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitReason {
    Admin,
    Size,
    Load,
    Other(i32),
}
impl Manager {
    /// Whether automatic splitting is permitted at the supplied Unix second.
    pub fn allow_split<C: SharedConfigProvider + ?Sized>(
        &self,
        labels: &RegionLabeler,
        conf: &C,
        region: Option<&Arc<RegionInfo>>,
        reason: SplitReason,
        now: u64,
    ) -> bool {
        let Some(r) = region else {
            return true;
        };
        if reason == SplitReason::Admin || conf.get_affinity_schedule_limit() == 0 {
            return true;
        }
        let size = conf.get_max_affinity_merge_region_size() as i64;
        if size == 0 {
            return true;
        }
        let (group, _) = self.get_region_affinity_group_state(labels, region, now);
        if group.is_some_and(|g| !g.regular_scheduling_allowed) {
            // Go int64 arithmetic wraps, including conversion from uint64 config.
            let max_size = size.wrapping_add(10).wrapping_mul(4);
            let max_keys = max_size.wrapping_mul(10_000);
            if r.stats().approximate_size < max_size && r.stats().approximate_keys < max_keys {
                return false;
            }
        }
        true
    }
}
