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
    async fn healthy(&self) -> Result<(), String>;
    async fn alloc_id(&self) -> Result<u64, String>;
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String>;
}
/// One router, listener and cancellation scope for HTTP/1 and HTTP/2 gRPC.
pub fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> axum::Router {
    tonic::service::Routes::new(astersql_pd_proto::pdpb::pd_server::PdServer::new(
        grpc::Grpc {
            services: services.clone(),
            cancel,
        },
    ))
    .into_axum_router()
    .merge(http::router(services))
}
