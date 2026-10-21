// Copyright 2026 AsterSQL.
//! Host-managed lifecycle for an empty PD node; no PD services are implemented.
mod node;
pub use node::{PdNode, PdNodeBuilder};

#[cfg(test)]
#[path = "node_test.rs"]
mod node_test;
