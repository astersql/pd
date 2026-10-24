// Copyright 2026 AsterSQL.

use std::{borrow::Cow, fmt};

/// A checker or scheduler identifier. Unknown strings remain valid identifiers.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckerSchedulerType(Cow<'static, str>);

impl CheckerSchedulerType {
    /// Constructs a static identifier without allocation.
    pub const fn from_static(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    /// Returns the exact identifier, including unknown values.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for CheckerSchedulerType {
    fn from(value: String) -> Self {
        Self(Cow::Owned(value))
    }
}

impl From<&str> for CheckerSchedulerType {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl fmt::Display for CheckerSchedulerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `joint-state-checker` identifier.
pub const JOINT_STATE_CHECKER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("joint-state-checker");
/// The `learner-checker` identifier.
pub const LEARNER_CHECKER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("learner-checker");
/// The `merge-checker` identifier.
pub const MERGE_CHECKER: CheckerSchedulerType = CheckerSchedulerType::from_static("merge-checker");
/// The `replica-checker` identifier.
pub const REPLICA_CHECKER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("replica-checker");
/// The `rule-checker` identifier.
pub const RULE_CHECKER: CheckerSchedulerType = CheckerSchedulerType::from_static("rule-checker");
/// The `split-checker` identifier.
pub const SPLIT_CHECKER: CheckerSchedulerType = CheckerSchedulerType::from_static("split-checker");
/// The `affinity-checker` identifier.
pub const AFFINITY_CHECKER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("affinity-checker");
/// The `split-scatter-checker` identifier.
pub const SPLIT_SCATTER_CHECKER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("split-scatter-checker");
/// The `balance-leader-scheduler` identifier.
pub const BALANCE_LEADER_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("balance-leader-scheduler");
/// The `balance-region-scheduler` identifier.
pub const BALANCE_REGION_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("balance-region-scheduler");
/// The `balance-witness-scheduler` identifier.
pub const BALANCE_WITNESS_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("balance-witness-scheduler");
/// The `evict-leader-scheduler` identifier.
pub const EVICT_LEADER_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("evict-leader-scheduler");
/// The `evict-slow-store-scheduler` identifier.
pub const EVICT_SLOW_STORE_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("evict-slow-store-scheduler");
/// The `evict-slow-trend-scheduler` identifier.
pub const EVICT_SLOW_TREND_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("evict-slow-trend-scheduler");
/// The `evict-stopping-store-scheduler` identifier.
pub const EVICT_STOPPING_STORE_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("evict-stopping-store-scheduler");
/// The `grant-leader-scheduler` identifier.
pub const GRANT_LEADER_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("grant-leader-scheduler");
/// The `grant-hot-region-scheduler` identifier.
pub const GRANT_HOT_REGION_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("grant-hot-region-scheduler");
/// The `balance-hot-region-scheduler` identifier.
pub const BALANCE_HOT_REGION_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("balance-hot-region-scheduler");
/// The `random-merge-scheduler` identifier.
pub const RANDOM_MERGE_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("random-merge-scheduler");
/// The `scatter-range-scheduler` identifier.
pub const SCATTER_RANGE_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("scatter-range-scheduler");
/// The `shuffle-hot-region-scheduler` identifier.
pub const SHUFFLE_HOT_REGION_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("shuffle-hot-region-scheduler");
/// The `shuffle-leader-scheduler` identifier.
pub const SHUFFLE_LEADER_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("shuffle-leader-scheduler");
/// The `shuffle-region-scheduler` identifier.
pub const SHUFFLE_REGION_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("shuffle-region-scheduler");
/// The `transfer-witness-leader-scheduler` identifier.
pub const TRANSFER_WITNESS_LEADER_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("transfer-witness-leader-scheduler");
/// The `label-scheduler` identifier.
pub const LABEL_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("label-scheduler");
/// The `balance-range-scheduler` identifier.
pub const BALANCE_RANGE_SCHEDULER: CheckerSchedulerType =
    CheckerSchedulerType::from_static("balance-range-scheduler");

/// All known checker identifiers; this is not a default checker configuration.
pub const CHECKERS: [CheckerSchedulerType; 8] = [
    JOINT_STATE_CHECKER,
    LEARNER_CHECKER,
    MERGE_CHECKER,
    REPLICA_CHECKER,
    RULE_CHECKER,
    SPLIT_CHECKER,
    AFFINITY_CHECKER,
    SPLIT_SCATTER_CHECKER,
];

/// Scheduler identifiers recognized by the canonical scheduler lookup.
pub const SCHEDULERS: [CheckerSchedulerType; 18] = [
    BALANCE_LEADER_SCHEDULER,
    BALANCE_REGION_SCHEDULER,
    BALANCE_WITNESS_SCHEDULER,
    EVICT_LEADER_SCHEDULER,
    EVICT_SLOW_STORE_SCHEDULER,
    EVICT_SLOW_TREND_SCHEDULER,
    EVICT_STOPPING_STORE_SCHEDULER,
    GRANT_LEADER_SCHEDULER,
    GRANT_HOT_REGION_SCHEDULER,
    BALANCE_HOT_REGION_SCHEDULER,
    RANDOM_MERGE_SCHEDULER,
    SCATTER_RANGE_SCHEDULER,
    SHUFFLE_HOT_REGION_SCHEDULER,
    SHUFFLE_LEADER_SCHEDULER,
    SHUFFLE_REGION_SCHEDULER,
    TRANSFER_WITNESS_LEADER_SCHEDULER,
    LABEL_SCHEDULER,
    BALANCE_RANGE_SCHEDULER,
];

/// Legacy persisted scheduler names paired with canonical identifiers.
pub const SCHEDULER_TYPE_COMPATIBLE_MAP: [(CheckerSchedulerType, &str); 18] = [
    (BALANCE_LEADER_SCHEDULER, "balance-leader"),
    (BALANCE_REGION_SCHEDULER, "balance-region"),
    (BALANCE_WITNESS_SCHEDULER, "balance-witness"),
    (EVICT_LEADER_SCHEDULER, "evict-leader"),
    (EVICT_SLOW_STORE_SCHEDULER, "evict-slow-store"),
    (EVICT_SLOW_TREND_SCHEDULER, "evict-slow-trend"),
    (EVICT_STOPPING_STORE_SCHEDULER, "evict-stopping-store"),
    (GRANT_LEADER_SCHEDULER, "grant-leader"),
    (GRANT_HOT_REGION_SCHEDULER, "grant-hot-region"),
    (BALANCE_HOT_REGION_SCHEDULER, "hot-region"),
    (RANDOM_MERGE_SCHEDULER, "random-merge"),
    (SCATTER_RANGE_SCHEDULER, "scatter-range"),
    (SHUFFLE_HOT_REGION_SCHEDULER, "shuffle-hot-region"),
    (SHUFFLE_LEADER_SCHEDULER, "shuffle-leader"),
    (SHUFFLE_REGION_SCHEDULER, "shuffle-region"),
    (TRANSFER_WITNESS_LEADER_SCHEDULER, "transfer-witness-leader"),
    (LABEL_SCHEDULER, "label"),
    (BALANCE_RANGE_SCHEDULER, "balance-range"),
];

/// Looks up a canonical scheduler string, including the historical scatter alias.
pub fn string_to_scheduler_type(value: &str) -> Option<CheckerSchedulerType> {
    if value == "scatter-range" {
        return Some(SCATTER_RANGE_SCHEDULER);
    }
    SCHEDULERS.into_iter().find(|t| t.as_str() == value)
}

/// Looks up a legacy persisted scheduler name exactly.
pub fn convert_old_str_to_type(value: &str) -> Option<CheckerSchedulerType> {
    SCHEDULER_TYPE_COMPATIBLE_MAP
        .into_iter()
        .find_map(|(t, old)| (old == value).then_some(t))
}

/// Returns the legacy persisted name of a known canonical scheduler identifier.
pub fn scheduler_type_compatible(value: &CheckerSchedulerType) -> Option<&'static str> {
    SCHEDULER_TYPE_COMPATIBLE_MAP
        .into_iter()
        .find_map(|(t, old)| (&t == value).then_some(old))
}

/// Returns whether the exact identifier is a known checker.
pub fn is_checker(value: &CheckerSchedulerType) -> bool {
    CHECKERS.contains(value)
}

/// Returns whether the exact identifier is a known canonical scheduler.
pub fn is_scheduler(value: &CheckerSchedulerType) -> bool {
    SCHEDULERS.contains(value)
}

/// Returns ordered scheduler defaults for an explicit kernel choice.
/// `is_next_gen` selects Next Generation when true, Classic when false.
pub fn default_schedulers(is_next_gen: bool) -> Vec<CheckerSchedulerType> {
    let mut defaults = vec![
        BALANCE_LEADER_SCHEDULER,
        BALANCE_REGION_SCHEDULER,
        BALANCE_HOT_REGION_SCHEDULER,
        EVICT_SLOW_STORE_SCHEDULER,
    ];
    if !is_next_gen {
        defaults.push(EVICT_STOPPING_STORE_SCHEDULER);
    }
    defaults
}

/// Returns defaults for the kernel selected by the canonical kerneltype crate.
pub fn current_kernel_default_schedulers() -> Vec<CheckerSchedulerType> {
    default_schedulers(astersql_pd_kerneltype::is_next_gen())
}
