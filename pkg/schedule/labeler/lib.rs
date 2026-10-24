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

mod labeler;
pub use labeler::{LabelerError, RegionLabeler};
#[cfg(test)]
#[path = "label_restore_test.rs"]
mod label_restore_test;

mod plan;
pub use plan::{LabelCommitOp, LabelPlan, LabelRulePatch};
#[cfg(test)]
#[path = "label_txn_test.rs"]
mod label_txn_test;

mod label_query;
#[cfg(test)]
#[path = "label_query_test.rs"]
mod label_query_test;

mod gc;
pub use gc::{LabelerGc, LabelerMetrics};
#[cfg(test)]
#[path = "label_gc_test.rs"]
mod label_gc_test;
