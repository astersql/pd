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

mod watch_reconcile;
pub use watch_reconcile::ReconciledWatch;
#[cfg(test)]
mod watch_reconcile_test;

mod watch_control;
pub use watch_control::{ControlledWatch, WatchControl};
#[cfg(test)]
mod watch_control_test;
