// Copyright 2026 AsterSQL.
//! Protocol-independent scheduling observations of canonical cluster state.

pub mod basic_cluster;
pub use basic_cluster::BasicCluster;

#[cfg(test)]
#[path = "task_002_test.rs"]
mod task_002_test;
