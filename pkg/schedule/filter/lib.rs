// Copyright 2026 AsterSQL.
//! Ordered scheduling filters over canonical store and configuration snapshots.
mod selection;
pub use selection::*;

#[cfg(test)]
#[path = "task_005_test.rs"]
mod task_005_test;
