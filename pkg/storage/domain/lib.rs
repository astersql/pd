// Copyright 2026 AsterSQL.
//! Storage contracts and byte/JSON operations, independent of protobuf and transports.
pub mod kv;
pub use kv::*;
mod endpoint;
pub use endpoint::{EndpointError, LoadState, StorageEndpoint};
#[path = "../endpoint/affinity_endpoint.rs"]
mod affinity_endpoint;
#[path = "../endpoint/batch_ops.rs"]
mod batch_ops;
#[path = "../endpoint/config_endpoint.rs"]
mod config_endpoint;
#[path = "../endpoint/rule_endpoint.rs"]
mod rule_endpoint;
#[path = "../endpoint/scheduler_endpoint.rs"]
mod scheduler_endpoint;
pub use batch_ops::{MAX_BATCH_OPS, run_batch_op_in_txn};
