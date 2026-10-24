// Copyright 2026 AsterSQL.
//! Rule selection for the shared region anomaly index.
use crate::{RegionAnomalyDurations, RegionStatistics};
use astersql_pd_core::{RegionInfo, StoreInfo};
use astersql_pd_schedule_config::CheckerConfigProvider;
use astersql_pd_schedule_placement::{PeerRoleType, RuleManager};

impl RegionStatistics {
    /// Observe using live placement rules; None means an uninitialized manager skipped observation.
    ///
    /// Configuration and manager are borrowed snapshots under the host's serialization.
    /// A skipped observation preserves every index entry and duration timestamp.
    pub fn observe_with_placement(
        &mut self,
        region: &RegionInfo,
        stores: &[StoreInfo],
        conf: &impl CheckerConfigProvider,
        manager: &RuleManager,
        now_unix_seconds: i64,
    ) -> Option<RegionAnomalyDurations> {
        let (replicas, voters) = if conf.is_placement_rules_enabled() {
            if !manager.is_initialized() {
                return None;
            }
            manager.get_rules_for_apply_region(region).iter().fold(
                (0_i64, 0_i64),
                |(replicas, voters), rule| {
                    (
                        replicas.wrapping_add(rule.count),
                        voters.wrapping_add(if rule.role == PeerRoleType::Learner {
                            0
                        } else {
                            rule.count
                        }),
                    )
                },
            )
        } else {
            (conf.get_max_replicas(), conf.get_max_replicas())
        };
        Some(self.observe_counts(region, stores, conf, now_unix_seconds, replicas, voters))
    }
}
