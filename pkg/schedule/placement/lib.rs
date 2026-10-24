// Copyright 2026 AsterSQL.
//! Placement rule values and store label selection, independent of protocols.
mod json;
pub mod label_constraint;
pub mod rule;
pub use label_constraint::{
    LabelConstraint, LabelConstraintOp, is_exclusive_label, match_label_constraints,
};
pub use rule::{GroupBundle, PeerRoleType, Rule, RuleGroup};
#[cfg(test)]
#[path = "placement_values_test.rs"]
mod placement_values_test;

pub mod rule_list;
pub use rule_list::{
    BuildRuleListError, RuleList, check_apply_rules, compare_rule, prepare_rules_for_apply,
};
#[cfg(test)]
#[path = "placement_order_test.rs"]
mod placement_order_test;

pub mod config;
pub use config::{RuleConfig, RuleConfigPatch, rule_may_restrict_engine};
#[cfg(test)]
#[path = "placement_patch_test.rs"]
mod placement_patch_test;

pub mod rule_manager;
pub use rule_manager::{RuleManager, RuleManagerError};
#[cfg(test)]
#[path = "placement_restore_test.rs"]
mod placement_restore_test;

pub mod rule_batch;
pub use rule_batch::RuleOp;
#[cfg(test)]
#[path = "placement_batch_test.rs"]
mod placement_batch_test;

pub mod fit;
pub use fit::{RuleFit, fit_rule};
#[cfg(test)]
#[path = "placement_role_fit_test.rs"]
mod placement_role_fit_test;

pub use fit::{RegionFit, fit_region};
#[cfg(test)]
#[path = "placement_region_fit_test.rs"]
mod placement_region_fit_test;

pub mod fit_isolation;
#[cfg(test)]
#[path = "placement_isolation_test.rs"]
mod placement_isolation_test;

pub mod region_rule_cache;
pub use region_rule_cache::RegionRuleFitCacheManager;
#[cfg(test)]
#[path = "placement_cache_test.rs"]
mod placement_cache_test;

#[cfg(test)]
#[path = "placement_manager_test.rs"]
mod placement_manager_test;
pub mod rule_query;
