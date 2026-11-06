// Copyright 2026 AsterSQL.
//! Scheduling RPC adaptation over the embedding host's canonical domain services.
mod service;
pub use service::{Host, Service};
mod scheduling_region_hb;

mod remote_ids;
pub use remote_ids::{PdLeaderLookup, RemoteIds};

#[cfg(test)]
mod service_test;
