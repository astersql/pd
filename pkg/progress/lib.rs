// Copyright 2026 AsterSQL.
//! Store progress values and fixed-interval speed windows.
#![forbid(unsafe_code)]
mod indicator;
pub use indicator::{InvalidUpdateInterval, Progress, ProgressIndicator};
#[cfg(test)]
mod task_130_test;
#[cfg(test)]
mod task_131_test;
