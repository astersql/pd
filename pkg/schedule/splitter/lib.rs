// Copyright 2026 AsterSQL.
//! Region split planning over canonical state.
mod split_keys;
pub use split_keys::*;
#[cfg(test)]
#[path = "task_062_test.rs"]
mod task_062_test;

mod split_results;
pub use split_results::*;
#[cfg(test)]
#[path = "task_063_test.rs"]
mod task_063_test;
