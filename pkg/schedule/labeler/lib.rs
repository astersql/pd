// Copyright 2026 AsterSQL.
#![forbid(unsafe_code)]
mod rules;
pub use rules::*;
#[cfg(test)]
#[path = "label_rule_test.rs"]
mod label_rule_test;
mod rule_index;
pub use rule_index::LabelRuleIndex;
#[cfg(test)]
#[path = "label_index_test.rs"]
mod label_index_test;
