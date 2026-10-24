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
