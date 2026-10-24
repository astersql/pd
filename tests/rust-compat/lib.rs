// Copyright 2026 AsterSQL.
//! Real-process differential harness for the Go oracle and future Rust services.
pub mod harness;
pub mod normalize;

#[cfg(test)]
mod schedule_foundation;
#[cfg(test)]
#[path = "schedule_foundation_test.rs"]
mod schedule_foundation_test;

#[cfg(test)]
mod state_regression;
#[cfg(test)]
mod state_regression_test;
