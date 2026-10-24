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
