// Copyright 2026 AsterSQL.
//! Consistent metadata snapshots over the canonical MetaStore contract.
mod snapshot_loader;
pub use snapshot_loader::{LoadedSnapshot, SnapshotLoader};
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[cfg(test)]
mod snapshot_loader_test;

mod snapshot_watch;
pub use snapshot_watch::SnapshotWatch;
#[cfg(test)]
mod snapshot_watch_test;
