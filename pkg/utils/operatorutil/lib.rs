// Copyright 2026 AsterSQL.
//! Operator comparisons for use as a dev-dependency by scheduling tests.
#![forbid(unsafe_code)]
mod assert_operator;
pub use assert_operator::*;
#[cfg(test)]
mod task_133_test;
