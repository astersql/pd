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

mod keyspace_index;
pub use keyspace_index::{
    KeyspaceRuleIndex, MAX_KEYSPACE_ID, merge_split_keys, parse_keyspace_rule_id,
};
#[cfg(test)]
#[path = "label_keyspace_test.rs"]
mod label_keyspace_test;
