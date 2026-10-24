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
