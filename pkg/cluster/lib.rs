// Copyright 2026 AsterSQL.
//! Shared cluster event bridges over host-owned state and scheduler queues.
#![forbid(unsafe_code)]
mod async_stats;
pub use async_stats::{AsyncStatsReceipt, WitnessAdmission, handle_stats_async};
#[cfg(test)]
mod task_127_test;

mod overlaps;
pub use overlaps::{OverlapsCancelled, clear_overlap_statistics, handle_overlaps};
#[cfg(test)]
mod task_128_test;

mod collect;
pub use collect::{CollectCancelled, collect};
#[cfg(test)]
mod task_129_test;
