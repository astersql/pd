// Copyright 2026 AsterSQL.
//! Shared cluster event bridges over host-owned state and scheduler queues.
#![forbid(unsafe_code)]
mod async_stats;
pub use async_stats::{AsyncStatsReceipt, WitnessAdmission, handle_stats_async};
#[cfg(test)]
mod task_127_test;
