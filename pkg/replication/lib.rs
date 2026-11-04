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

mod async_wait;
#[cfg(test)]
mod async_wait_test;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;

mod async_confirm;
pub use async_confirm::StoreDRStatus;
#[cfg(test)]
mod async_confirm_test;
