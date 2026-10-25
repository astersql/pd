// Copyright 2026 AsterSQL.
//! Rule boundary splits over canonical placement and persisted label snapshots.
use crate::PauseController;
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{BuilderError, OpKind, Operator, create_split_region_operator};
use astersql_pd_schedule_placement::RuleManager;

/// Prefer label boundaries, falling back to placement only when enabled.
#[derive(Default, Debug)]
pub struct SplitChecker {
    pub pause: PauseController,
}
impl SplitChecker {
    /// Stable checker name used by host pause/control dispatch.
    pub fn get_type(&self) -> &'static str {
        "split-checker"
    }

    /// Read immutable host snapshots and construct one USEKEY split operation.
    /// A factory failure yields no operation and retains its typed diagnostic;
    /// Go logs and discards that error. The host supplies Unix seconds for pause.
    pub fn check<C: SharedConfigProvider>(
        &self,
        conf: &C,
        rules: &RuleManager,
        labels: &RegionLabeler,
        region: &RegionInfo,
        now_unix_seconds: i64,
    ) -> Result<Option<Operator>, BuilderError> {
        if self.pause.is_paused_at(now_unix_seconds) {
            return Ok(None);
        }
        let range = &region.meta().key_range;
        let mut description = "labeler-split-region";
        let mut keys: Vec<Vec<u8>> = labels
            .index()
            .get_split_keys(&range.start_key, &range.end_key)
            .into_iter()
            .map(<[u8]>::to_vec)
            .collect();
        if keys.is_empty() && conf.is_placement_rules_enabled() {
            description = "rule-split-region";
            keys = rules.get_split_keys(&range.start_key, &range.end_key);
        }
        if keys.is_empty() {
            return Ok(None);
        }
        create_split_region_operator(
            description,
            region,
            OpKind::SPLIT,
            SplitPolicy::UseKey,
            keys,
        )
        .map(Some)
    }
}
