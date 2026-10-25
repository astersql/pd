// Copyright 2026 AsterSQL.
//! Single-region scatter target selection over host-owned scheduling snapshots.
#![forbid(unsafe_code)]
mod scatter_target;
pub use scatter_target::*;
#[cfg(test)]
mod task_059_test;
