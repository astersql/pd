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
#[path = "../endpoint/replication_endpoint.rs"]
mod replication_endpoint;
#[path = "../endpoint/rule_endpoint.rs"]
mod rule_endpoint;
#[path = "../endpoint/scheduler_endpoint.rs"]
mod scheduler_endpoint;
pub use batch_ops::{MAX_BATCH_OPS, run_batch_op_in_txn};

mod hot_history;
pub use hot_history::{HistoryCipher, HistoryError, HistoryHotRegion, HistoryQuery, HotHistory};

mod meta_service_group;
pub use meta_service_group::MetaServiceGroupStatus;

mod keyspace_group_value;
pub use keyspace_group_value::{KeyspaceGroupMember, UserKind};
mod keyspace_group_state;
pub use keyspace_group_state::{KeyspaceGroup, MergeState, OptionalKeyspaceGroupState, SplitState};
mod keyspace_group_store;
pub use keyspace_group_store::KeyspaceGroupStoreError;

mod keyspace_meta;
pub use keyspace_meta::{
    Keyspace, KeyspaceCodec, KeyspaceIdentity, KeyspaceMetaError, KeyspaceStorage,
};
