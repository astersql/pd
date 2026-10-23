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
