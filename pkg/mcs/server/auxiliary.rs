// Copyright 2026 AsterSQL.
//! Common listener services registered by Go StartGRPCAndHTTPServers.
use crate::DiagnosticsService;
use astersql_pd_proto::{
    diagnosticspb::diagnostics_server::DiagnosticsServer,
    health::{
        HealthCheckRequest, HealthCheckResponse, HealthListRequest, HealthListResponse,
        health_server::{Health, HealthServer},
    },
};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
/// Assemble the common auxiliary services on the host's executor and stop scope.
/// Go registers only the empty health service as SERVING, including List; this
/// is listener health rather than domain leadership or bootstrap readiness.
pub fn auxiliary_routes(
    runtime: tokio::runtime::Handle,
    cancel: CancellationToken,
    log_file: String,
) -> tonic::service::Routes {
    tonic::service::Routes::new(DiagnosticsServer::new(DiagnosticsService::new(
        runtime,
        cancel.clone(),
        Arc::new(move || log_file.clone()),
    )))
    .add_service(HealthServer::new(ListenerHealth { cancel }))
}
struct ListenerHealth {
    cancel: CancellationToken,
}
#[tonic::async_trait]
impl Health for ListenerHealth {
    async fn check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        if request.into_inner().service.is_empty() {
            Ok(Response::new(HealthCheckResponse { status: 1 }))
        } else {
            Err(Status::not_found("unknown service"))
        }
    }
    async fn list(
        &self,
        _: Request<HealthListRequest>,
    ) -> Result<Response<HealthListResponse>, Status> {
        Ok(Response::new(HealthListResponse {
            statuses: [(String::new(), HealthCheckResponse { status: 1 })].into(),
        }))
    }
    type WatchStream = tonic::codegen::BoxStream<HealthCheckResponse>;
    async fn watch(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let status = if request.into_inner().service.is_empty() {
            1
        } else {
            3
        };
        let cancel = self.cancel.clone();
        // No producer task or per-client state outlives the demand-driven stream.
        Ok(Response::new(Box::pin(
            futures_util::stream::once(async move { Ok(HealthCheckResponse { status }) }).chain(
                futures_util::stream::once(async move {
                    cancel.cancelled().await;
                    Err(Status::cancelled("context canceled"))
                }),
            ),
        )))
    }
}
