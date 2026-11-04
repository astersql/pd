// Copyright 2026 AsterSQL.
//! Replication policy and durable mode state; protocols stay in adapters.
mod config;
pub use config::*;
mod mode_state;
pub use mode_state::*;
#[cfg(test)]
mod mode_proxy_test;
#[cfg(test)]
mod mode_state_test;
