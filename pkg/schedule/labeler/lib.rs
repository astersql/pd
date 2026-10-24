// Copyright 2026 AsterSQL.
#![forbid(unsafe_code)]
mod rules;
pub use rules::*;
#[cfg(test)]
#[path = "label_rule_test.rs"]
mod label_rule_test;
