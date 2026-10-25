// Copyright 2026 AsterSQL.
//! Protocol-independent scheduling observations of canonical cluster state.

pub mod basic_cluster;
pub mod shared_cluster;
pub use basic_cluster::BasicCluster;
pub use shared_cluster::{
    IdAllocation, PrepareRegionCount, SharedCluster, SharedClusterView, SharedIdAllocator,
};

#[cfg(test)]
#[path = "task_002_test.rs"]
mod task_002_test;

#[cfg(test)]
#[path = "task_003_test.rs"]
mod task_003_test;
