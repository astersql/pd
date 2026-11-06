// Copyright 2026 AsterSQL.
//! Router transport over the host's canonical cluster cache.
mod router_query;
pub use router_query::Service;

mod service_host;
pub use service_host::{HostConfig, RouterHost};

#[cfg(test)]
mod router_query_test;
