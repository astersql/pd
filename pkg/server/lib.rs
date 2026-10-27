// Copyright 2026 AsterSQL.
//! HTTP and gRPC adapters for the minimal classic PD role.
mod grpc;
mod http;
use astersql_pd_tso::Timestamp;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Adapter-independent single-node identity. Multi-member forwarding is not yet supported.
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
    async fn healthy(&self) -> Result<(), String>;
    async fn alloc_id(&self) -> Result<u64, String>;
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String>;
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
