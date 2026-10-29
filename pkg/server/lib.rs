// Copyright 2026 AsterSQL.
//! HTTP and gRPC adapters for the minimal classic PD role.
mod grpc;
mod grpc_alloc_id;
pub mod grpc_ask_split;
mod grpc_bootstrap;
mod grpc_cluster_config;
mod grpc_members;
mod grpc_region_lookup;
mod grpc_region_query;
mod grpc_region_scan;
mod grpc_report_split;
pub mod grpc_scatter;
pub mod grpc_split_regions;
pub mod grpc_store;
mod http;
mod request_role;
use astersql_pd_tso::Timestamp;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Adapter-independent local identity. Discovery reads the shared member registry.
#[derive(Clone)]
pub struct Identity {
    pub cluster_id: u64,
    pub member_id: u64,
    pub name: String,
    pub client_url: String,
    pub peer_url: String,
}
/// Domain operations supplied by the host; transport types stay in this crate.
#[async_trait]
pub trait PdServices: Send + Sync {
    fn identity(&self) -> &Identity;
    /// Optional store state service supplied by the host.
    fn store_heartbeat_service(
        &self,
    ) -> Option<&dyn store_heartbeat_adapter::StoreHeartbeatService> {
        None
    }
    /// Optional region state service supplied by the host.
    fn region_heartbeat_service(
        &self,
    ) -> Option<&dyn region_heartbeat_adapter::RegionHeartbeatService> {
        None
    }
    /// Optional bucket state service supplied by the host.
    fn bucket_heartbeat_service(
        &self,
    ) -> Option<&dyn bucket_heartbeat_adapter::BucketHeartbeatService> {
        None
    }
    /// Optional host-owned region synchronization transport.
    fn region_sync_service(&self) -> Option<&sync_transport::RegionSyncer> {
        None
    }
    /// Owned term handle for hosts whose leader service can change concurrently.
    fn region_sync_handle(&self) -> Option<Arc<sync_transport::RegionSyncer>> {
        None
    }
    /// Canonical metadata reads used by startup recovery verification.
    fn cluster_read_service(&self) -> Option<&dyn ClusterReadService> {
        None
    }
    /// Store configuration gate, matching Go's disabled-by-default region buckets.
    fn region_buckets_enabled(&self) -> bool {
        false
    }
    /// Local synchronized follower view, separate from the scheduling leader term.
    fn follower_read_service(&self) -> Option<&dyn ClusterReadService> {
        None
    }
    /// Whether this host currently exposes a bootstrapped raft cluster.
    /// Implementations must read canonical state, rather than infer it from requests.
    async fn is_bootstrapped(&self) -> Result<bool, String> {
        Ok(true)
    }
    /// Shared canonical discovery metadata, independent of the local leader term.
    fn discovery_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        None
    }
    /// Metadata backend fenced to the active leader term for configuration writes.
    fn cluster_config_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        self.discovery_store()
    }
    /// Commit initial metadata and publish it into the canonical host cache.
    async fn bootstrap(
        &self,
        _store: astersql_pd_core::StoreMeta,
        _region: astersql_pd_core::RegionMeta,
    ) -> Result<bool, String> {
        Err("bootstrap is not configured".into())
    }
    /// Persist registration into the host-owned canonical cluster.
    async fn put_store(
        &self,
        _store: astersql_pd_core::StoreMeta,
    ) -> Result<(), grpc_store::StoreError> {
        Err(grpc_store::StoreError::Other(
            "store registration is not configured".into(),
        ))
    }
    /// List metadata from the same view used by heartbeats and GetStore.
    async fn stores(&self) -> Result<Vec<astersql_pd_core::StoreMeta>, String> {
        Err("store listing is not configured".into())
    }
    async fn split_regions(
        &self,
        _request: grpc_split_regions::SplitRequest,
    ) -> Result<grpc_split_regions::SplitResult, String> {
        Err("splitting is not configured".into())
    }
    async fn scatter(
        &self,
        _request: grpc_scatter::ScatterRequest,
    ) -> Result<grpc_scatter::ScatterResult, grpc_scatter::ScatterError> {
        Err(grpc_scatter::ScatterError::Other(
            "scheduling is not configured".into(),
        ))
    }
    async fn operator(&self, _id: u64) -> Result<Option<grpc_scatter::OperatorResult>, String> {
        Err("scheduling is not configured".into())
    }
    async fn healthy(&self) -> Result<(), String>;
    async fn alloc_id(&self) -> Result<u64, String>;
    /// Reserve one indivisible batch and return its last ID and count.
    async fn alloc_ids(&self, count: u32) -> Result<(u64, u32), String> {
        if count != 1 {
            return Err("batch ID allocation is not configured".into());
        }
        self.alloc_id().await.map(|id| (id, 1))
    }
    /// An unsupported cluster version, or None when batch splitting is supported.
    async fn batch_split_version(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    /// Validate and allocate against the same term-owned cluster state.
    async fn split_ids(
        &self,
        _region: astersql_pd_core::RegionMeta,
        _count: u32,
        _batch: bool,
        _reason: i32,
    ) -> Result<grpc_ask_split::SplitIds, grpc_ask_split::SplitError> {
        Err(grpc_ask_split::SplitError::Other(
            "split allocation is not configured".into(),
        ))
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String>;
}
/// Host-owned metadata lookup; protocol conversion remains in the server adapter.
#[async_trait]
pub trait ClusterReadService: Send + Sync {
    /// None means the follower synchronization worker is not running.
    async fn query_regions(
        &self,
        _keys: &[Vec<u8>],
        _prev_keys: &[Vec<u8>],
        _ids: &[u64],
        _need_buckets: bool,
    ) -> Result<Option<astersql_pd_core::RegionQueryResult>, String> {
        Err("region queries are not configured".into())
    }

    async fn batch_scan_regions(
        &self,
        _ranges: &[astersql_pd_core::KeyRange],
        _options: astersql_pd_core::BatchScanRegionsOptions,
    ) -> Result<
        Result<Vec<Arc<astersql_pd_core::RegionInfo>>, astersql_pd_core::RegionNotAdjacent>,
        String,
    > {
        Err("batch scan is not configured".into())
    }
    async fn store(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::StoreInfo>>, String>;
    async fn region(&self, id: u64) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String>;
    async fn scan_regions(
        &self,
        _start: &[u8],
        _end: &[u8],
        _limit: isize,
    ) -> Result<Vec<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region scan is not configured".into())
    }
    async fn region_by_key(
        &self,
        _key: &[u8],
        _previous: bool,
    ) -> Result<Option<Arc<astersql_pd_core::RegionInfo>>, String> {
        Err("region key lookup is not configured".into())
    }
}
/// One router, listener and cancellation scope for HTTP/1 and HTTP/2 gRPC.
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {
    tonic::service::Routes::new(astersql_pd_proto::pdpb::pd_server::PdServer::new(
        sync_transport::SyncGrpc(grpc::Grpc {
            services: services.clone(),
            cancel,
        }),
    ))
    .into_axum_router()
    .merge(http::router(services))
}

pub mod core_trace;
#[cfg(test)]
#[path = "core_trace_test.rs"]
mod core_trace_test;

pub mod core_store_adapter;
#[cfg(test)]
#[path = "core_store_adapter_test.rs"]
mod core_store_adapter_test;

pub mod core_region_adapter;
#[cfg(test)]
#[path = "core_region_adapter_test.rs"]
mod core_region_adapter_test;

pub mod store_heartbeat_adapter;
#[cfg(test)]
#[path = "store_heartbeat_adapter_test.rs"]
mod store_heartbeat_adapter_test;

pub mod region_heartbeat_adapter;
#[cfg(test)]
#[path = "region_heartbeat_adapter_test.rs"]
mod region_heartbeat_adapter_test;

pub mod bucket_heartbeat_adapter;
#[cfg(test)]
#[path = "bucket_heartbeat_adapter_test.rs"]
mod bucket_heartbeat_adapter_test;

pub mod sync_codec;
#[cfg(test)]
#[path = "sync_codec_test.rs"]
mod sync_codec_test;

pub mod sync_transport;
#[cfg(test)]
#[path = "sync_transport_test.rs"]
mod sync_transport_test;

pub mod schedule_wire;
#[cfg(test)]
#[path = "schedule_wire_test.rs"]
mod schedule_wire_test;

pub mod schedule_admin_balance_leader;
#[cfg(test)]
#[path = "schedule_admin_balance_leader_test.rs"]
mod schedule_admin_balance_leader_test;

pub mod schedule_admin_balance_region;
#[cfg(test)]
#[path = "schedule_admin_balance_region_test.rs"]
mod schedule_admin_balance_region_test;

pub mod schedule_admin_balance_witness;
#[cfg(test)]
#[path = "schedule_admin_balance_witness_test.rs"]
mod schedule_admin_balance_witness_test;

pub mod schedule_admin_evict_leader;
#[cfg(test)]
#[path = "schedule_admin_evict_leader_test.rs"]
mod schedule_admin_evict_leader_test;

pub mod schedule_admin_evict_slow_store;
#[cfg(test)]
#[path = "schedule_admin_evict_slow_store_test.rs"]
mod schedule_admin_evict_slow_store_test;

pub mod schedule_admin_evict_stopping_store;
#[cfg(test)]
#[path = "schedule_admin_evict_stopping_store_test.rs"]
mod schedule_admin_evict_stopping_store_test;

pub mod schedule_admin_grant_hot_region;
#[cfg(test)]
#[path = "schedule_admin_grant_hot_region_test.rs"]
mod schedule_admin_grant_hot_region_test;

pub mod schedule_admin_hot_region;
#[cfg(test)]
#[path = "schedule_admin_hot_region_test.rs"]
mod schedule_admin_hot_region_test;

pub mod schedule_admin_grant_leader;
#[cfg(test)]
#[path = "schedule_admin_grant_leader_test.rs"]
mod schedule_admin_grant_leader_test;

// Label scheduler inherited configuration endpoint.
pub mod schedule_admin_label;
#[cfg(test)]
#[path = "schedule_admin_label_test.rs"]
mod schedule_admin_label_test;

pub mod schedule_admin_random_merge;
#[cfg(test)]
#[path = "schedule_admin_random_merge_test.rs"]
mod schedule_admin_random_merge_test;

pub mod schedule_admin_scatter_range;
#[cfg(test)]
#[path = "schedule_admin_scatter_range_test.rs"]
mod schedule_admin_scatter_range_test;

pub mod schedule_admin_shuffle_hot_region;
#[cfg(test)]
#[path = "schedule_admin_shuffle_hot_region_test.rs"]
mod schedule_admin_shuffle_hot_region_test;

pub mod schedule_admin_shuffle_leader;
#[cfg(test)]
#[path = "schedule_admin_shuffle_leader_test.rs"]
mod schedule_admin_shuffle_leader_test;

pub mod schedule_admin_shuffle_region;
#[cfg(test)]
#[path = "schedule_admin_shuffle_region_test.rs"]
mod schedule_admin_shuffle_region_test;

pub mod schedule_admin_transfer_witness_leader;
#[cfg(test)]
#[path = "schedule_admin_transfer_witness_leader_test.rs"]
mod schedule_admin_transfer_witness_leader_test;

pub mod schedule_admin_evict_slow_trend;
#[cfg(test)]
#[path = "schedule_admin_evict_slow_trend_test.rs"]
mod schedule_admin_evict_slow_trend_test;

pub mod schedule_admin_balance_range;
#[cfg(test)]
#[path = "schedule_admin_balance_range_test.rs"]
mod schedule_admin_balance_range_test;

mod heartbeat_stream_adapter;

#[cfg(test)]
#[path = "request_role_test.rs"]
mod request_role_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;

#[cfg(test)]
#[path = "grpc_members_test.rs"]
mod grpc_members_test;

#[cfg(test)]
#[path = "grpc_bootstrap_test.rs"]
mod grpc_bootstrap_test;

#[cfg(test)]
#[path = "grpc_alloc_id_test.rs"]
mod grpc_alloc_id_test;

#[cfg(test)]
#[path = "grpc_store_test.rs"]
mod grpc_store_test;

#[cfg(test)]
#[path = "grpc_store_scenarios_test.rs"]
mod grpc_store_scenarios_test;

#[cfg(test)]
#[path = "grpc_region_lookup_test.rs"]
mod grpc_region_lookup_test;

#[cfg(test)]
#[path = "grpc_region_lookup_scenarios_test.rs"]
mod grpc_region_lookup_scenarios_test;

#[cfg(test)]
#[path = "grpc_region_scan_test.rs"]
mod grpc_region_scan_test;

#[cfg(test)]
#[path = "grpc_region_scan_scenarios_test.rs"]
mod grpc_region_scan_scenarios_test;

#[cfg(test)]
mod grpc_region_query_scenarios_test;
#[cfg(test)]
mod grpc_region_query_test;

#[cfg(test)]
#[path = "grpc_ask_split_scenarios_test.rs"]
mod grpc_ask_split_scenarios_test;
#[cfg(test)]
#[path = "grpc_ask_split_test.rs"]
mod grpc_ask_split_test;

#[cfg(test)]
#[path = "grpc_report_split_test.rs"]
mod grpc_report_split_test;

#[cfg(test)]
mod grpc_cluster_config_test;

#[cfg(test)]
#[path = "grpc_scatter_test.rs"]
mod grpc_scatter_test;

#[cfg(test)]
mod grpc_scatter_scenarios_test;

#[cfg(test)]
mod grpc_split_regions_test;

#[cfg(test)]
pub(crate) mod grpc_split_regions_scenarios_test;
