// Copyright 2026 AsterSQL.
//! Scheduling facade admission over canonical cluster state.
pub mod prepare;
pub use prepare::{COLLECT_TIMEOUT, PrepareChecker};

#[cfg(test)]
#[path = "task_112_test.rs"]
mod task_112_test;

pub mod coordinator;
pub use coordinator::*;
#[cfg(test)]
#[path = "task_113_test.rs"]
mod task_113_test;

pub mod scheduler_restore;
pub use scheduler_restore::*;
#[cfg(test)]
#[path = "task_114_test.rs"]
mod task_114_test;
