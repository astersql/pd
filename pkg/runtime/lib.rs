// Copyright 2026 AsterSQL.
//! Host-managed lifecycle for the minimal classic PD role.
mod node;
pub use node::{NodeConfig, NodeError, PdNode, PdNodeBuilder, bootstrap_config};

#[cfg(test)]
#[path = "node_test.rs"]
mod node_test;

pub mod cluster_host;
#[cfg(test)]
#[path = "cluster_host_test.rs"]
mod cluster_host_test;
