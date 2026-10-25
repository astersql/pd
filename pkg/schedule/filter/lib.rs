// Copyright 2026 AsterSQL.
//! Ordered scheduling filters over canonical store and configuration snapshots.
mod candidates;
pub use candidates::*;
mod selection;
pub use selection::*;

#[cfg(test)]
#[path = "task_005_test.rs"]
mod task_005_test;

#[cfg(test)]
#[path = "task_006_test.rs"]
mod task_006_test;
