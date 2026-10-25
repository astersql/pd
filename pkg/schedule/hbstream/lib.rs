// Copyright 2026 AsterSQL.
//! Protocol independent heartbeat commands. Stream lifecycle is added separately.
pub mod operation;
pub use operation::*;

#[cfg(test)]
#[path = "task_012_test.rs"]
mod task_012_test;
