// Copyright 2026 AsterSQL.
//! Protocol independent heartbeat commands and serial stream routing.
pub mod operation;
pub mod routing;
pub use operation::*;

#[cfg(test)]
#[path = "task_012_test.rs"]
mod task_012_test;

#[cfg(test)]
#[path = "task_013_test.rs"]
mod task_013_test;
