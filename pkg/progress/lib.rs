// Copyright 2026 AsterSQL.
//! Store progress, dynamic speed windows, and completed-store collection.
#![forbid(unsafe_code)]
mod indicator;
pub use indicator::{InvalidUpdateInterval, Progress, ProgressIndicator};
#[cfg(test)]
mod task_130_test;
#[cfg(test)]
mod task_131_test;

mod progress_manager;
pub use progress_manager::Manager;
#[cfg(test)]
mod task_132_test;
