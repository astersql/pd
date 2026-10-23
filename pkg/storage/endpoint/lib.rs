// Copyright 2026 AsterSQL.
//! Persistence adapters over the canonical backend-neutral MetaStore.
mod endpoint_codec;
/// Protobuf persistence adapter; protocol types remain outside domain interfaces.
pub use endpoint_codec::protobuf;
pub use endpoint_codec::{EndpointError, LoadState, StorageEndpoint};
#[cfg(test)]
mod endpoint_codec_test;

mod cluster_id;
pub use cluster_id::{ClusterIdError, cluster_id_candidate};
#[cfg(test)]
mod cluster_id_test;

mod cluster_meta;
pub use cluster_meta::Cluster;
#[cfg(test)]
mod cluster_meta_test;

mod store_meta;
#[cfg(test)]
mod store_meta_test;

mod store_weights;
#[cfg(test)]
mod store_weights_test;

mod region_meta;
pub use region_meta::{RegionLoadError, RegionMetaError};
#[cfg(test)]
mod region_meta_test;

mod region_scan;
pub use region_scan::RegionScanError;
#[cfg(test)]
mod region_scan_test;

mod config_endpoint;
#[cfg(test)]
mod config_endpoint_test;

mod scheduler_endpoint;
#[cfg(test)]
mod scheduler_endpoint_test;

mod rule_endpoint;
#[cfg(test)]
mod rule_endpoint_test;

mod affinity_endpoint;
#[cfg(test)]
mod affinity_endpoint_test;

mod replication_endpoint;
#[cfg(test)]
mod replication_endpoint_test;

mod timestamp_values;
pub use timestamp_values::TimestampValueError;
#[cfg(test)]
mod timestamp_values_test;

mod timestamp_window;
pub use timestamp_window::TimestampWindowError;
#[cfg(test)]
mod timestamp_window_test;

mod timestamp_fenced_save;
pub use timestamp_fenced_save::TimestampSaveError;
#[cfg(test)]
mod timestamp_fenced_save_test;

mod resource_settings;
#[cfg(test)]
mod resource_settings_test;

mod resource_states;
#[cfg(test)]
mod resource_states_test;

mod service_limits;
#[cfg(test)]
mod service_limits_test;

mod keyspace_meta;
pub use keyspace_meta::{Keyspace, KeyspaceIdentity, KeyspaceMetaError};
#[cfg(test)]
mod keyspace_meta_test;

#[cfg(test)]
mod keyspace_scan_test;

mod keyspace_group_value;
pub use keyspace_group_value::{KeyspaceGroupMember, UserKind};
#[cfg(test)]
mod keyspace_group_value_test;

mod keyspace_group_state;
pub use keyspace_group_state::{KeyspaceGroup, MergeState, OptionalKeyspaceGroupState, SplitState};
#[cfg(test)]
mod keyspace_group_state_test;

mod keyspace_group_store;
pub use keyspace_group_store::KeyspaceGroupStoreError;
#[cfg(test)]
mod keyspace_group_store_test;

mod meta_service_group;
pub use meta_service_group::MetaServiceGroupStatus;
#[cfg(test)]
mod meta_service_group_test;

mod maintenance_start;
pub use maintenance_start::{MAINTENANCE_LOCK_NAME, MaintenanceStartError, MaintenanceTask};
#[cfg(test)]
mod maintenance_start_test;

mod maintenance_delete;
#[cfg(test)]
mod maintenance_delete_test;

mod gc_barrier_value;
pub use gc_barrier_value::{BarrierTime, GCBarrier, GlobalGCBarrier, ServiceSafePoint};
#[cfg(test)]
mod gc_barrier_value_test;
