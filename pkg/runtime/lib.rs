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

#[cfg(test)]
#[path = "cluster_restore_test.rs"]
mod cluster_restore_test;

mod cluster_restore;

mod member_role_worker;
#[cfg(test)]
mod member_role_worker_test;

#[cfg(test)]
mod wire_store_heartbeat_test;

#[cfg(test)]
mod wire_region_heartbeat_test;

#[cfg(test)]
mod wire_bucket_heartbeat_test;

#[cfg(test)]
mod wire_sync_regions_test;

#[cfg(test)]
mod schedule_worker_test;

#[cfg(test)]
mod request_role_test;

#[cfg(test)]
mod grpc_members_differential_test;

#[cfg(test)]
mod grpc_bootstrap_differential_test;

#[cfg(test)]
mod grpc_alloc_id_differential_test;
