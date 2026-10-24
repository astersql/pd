// Copyright 2026 AsterSQL.
//! Region synchronization domain primitives.
mod history_buffer;
pub use history_buffer::{
    HistoryBuffer, HistoryError, HistoryIndex, normalize_history_buffer_capacity,
};

#[cfg(test)]
#[path = "history_buffer_test.rs"]
mod history_buffer_test;

mod history_window;
pub use history_window::HistoryRetain;
#[cfg(test)]
#[path = "history_window_test.rs"]
mod history_window_test;

mod history_persist;
pub use history_persist::{HISTORY_KEY, HistoryPersistError, PersistentHistory};
#[cfg(test)]
#[path = "history_persist_test.rs"]
mod history_persist_test;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod persist_fixture;

mod history_replay;
pub use history_replay::{HistoryReplay, MAX_SYNC_REGION_BATCH_SIZE, SyncFrame};
#[cfg(test)]
#[path = "history_replay_test.rs"]
mod history_replay_test;

mod full_snapshot;
pub use full_snapshot::{FullSnapshot, SnapshotBatch, SnapshotOffset};
#[cfg(test)]
#[path = "full_snapshot_test.rs"]
mod full_snapshot_test;

mod full_catchup;
pub use full_catchup::{CatchUpFrame, FullCatchUpError};
#[cfg(test)]
#[path = "full_catchup_test.rs"]
mod full_catchup_test;

mod downstream;
pub use downstream::{
    BootstrapFrame, DownstreamError, DownstreamSender, DownstreamTask, Downstreams,
};
#[cfg(test)]
#[path = "downstream_test.rs"]
mod downstream_test;

mod client_apply;
pub use client_apply::{
    ApplyHistory, ApplyOutcome, ClientApply, SyncApplyResponse, SyncRegionStats, SyncResponseError,
};
#[cfg(test)]
#[path = "client_apply_test.rs"]
mod client_apply_test;

mod client_ready;
#[cfg(test)]
#[path = "client_ready_test.rs"]
mod client_ready_test;

mod metadata_watch;
pub use metadata_watch::MetadataWatch;
#[cfg(test)]
#[path = "metadata_watch_test.rs"]
mod metadata_watch_test;

mod lifecycle;
pub use lifecycle::{
    SyncAttempt, SyncAttemptOutcome, SyncLifecycle, SyncLifecycleError, SyncMetrics,
};
#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;
