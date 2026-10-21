// Copyright 2026 AsterSQL.
//! Disposable Raft/disk experiment. Not a production MetaStore implementation.
#![cfg(feature = "prototype")]
mod log_store;
mod node;
mod state_machine;
pub use node::Cluster;
