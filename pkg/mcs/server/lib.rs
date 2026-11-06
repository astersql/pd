// Copyright 2026 AsterSQL.
//! Shared MCS listener ownership over the caller's executor and transport.
mod base_listener;
mod cancellable_io;
pub use base_listener::{BaseServer, StartCallback};
#[cfg(test)]
mod base_listener_test;
