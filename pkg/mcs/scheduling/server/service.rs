// Copyright 2026 AsterSQL.
use astersql_pd_core::NodeState;
use astersql_pd_proto::schedulingpb::{self as p, scheduling_server::Scheduling};
use astersql_pd_server::{
    PdServices,
    grpc_ask_split::SplitError,
    grpc_scatter::{ScatterError, ScatterRequest, operator_status},
    grpc_split_regions::SplitRequest,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
/// Supplies only the current initialized scheduling term. The service never
/// constructs a PD node, coordinator, cache, allocator, or executor.
pub trait Host: Send + Sync {
    fn cluster_id(&self) -> u64;
    fn cluster(&self) -> Option<Arc<dyn PdServices>>;
    fn force_load_stores(&self);
}
#[derive(Clone)]
pub struct Service {
    pub(crate) host: Arc<dyn Host>,
    pub(crate) cancel: CancellationToken,
}
impl Service {
    pub fn new(host: Arc<dyn Host>, cancel: CancellationToken) -> Self {
        Self { host, cancel }
    }
    pub(crate) fn header(&self) -> p::ResponseHeader {
        p::ResponseHeader {
            cluster_id: self.host.cluster_id(),
            error: None,
        }
    }
    pub(crate) fn error(
        &self,
        kind: p::ErrorType,
        message: impl Into<String>,
    ) -> p::ResponseHeader {
        p::ResponseHeader {
            cluster_id: self.host.cluster_id(),
            error: Some(p::Error {
                r#type: kind as i32,
                message: message.into(),
            }),
        }
    }
    pub(crate) fn unavailable_cluster(&self) -> p::ResponseHeader {
        self.error(p::ErrorType::NotBootstrapped, "cluster is not initialized")
    }
}
#[tonic::async_trait]
impl Scheduling for Service {
    async fn store_heartbeat(
        &self,
        request: Request<p::StoreHeartbeatRequest>,
    ) -> Result<Response<p::StoreHeartbeatResponse>, Status> {
        let Some(cluster) = self.host.cluster() else {
            return Ok(Response::new(p::StoreHeartbeatResponse {
                header: Some(self.unavailable_cluster()),
                ..Default::default()
            }));
        };
        let stats = astersql_pd_grpcutil::core_store_adapter::store_stats_from_proto(
            request.into_inner().stats,
        )
        .unwrap_or_default();
        let id = stats.store_id;
        let region = cluster.region_heartbeat_service().ok_or_else(|| {
            Status::unimplemented("region heartbeat state service is not configured")
        })?;
        let state = region.region_store_state(id);
        if state.is_none() {
            self.host.force_load_stores();
        }
        if state.is_none() || state == Some(NodeState::Removed) {
            let message = if state.is_none() {
                format!("store {id} not found")
            } else {
                format!("store {id} is tombstone")
            };
            return Ok(Response::new(p::StoreHeartbeatResponse {
                header: Some(self.error(p::ErrorType::Unknown, message)),
                ..Default::default()
            }));
        }
        let service = cluster.store_heartbeat_service().ok_or_else(|| {
            Status::unimplemented("store heartbeat state service is not configured")
        })?;
        // Go reports the admission outcome through metrics but returns success
        // after the initial membership check, including handler failures.
        tokio::select! {biased;_=self.cancel.cancelled()=>return Err(Status::unavailable("server closed")),_=service.store_heartbeat(stats)=>{}}
        Ok(Response::new(p::StoreHeartbeatResponse {
            header: Some(self.header()),
            ..Default::default()
        }))
    }
    async fn split_regions(
        &self,
        request: Request<p::SplitRegionsRequest>,
    ) -> Result<Response<p::SplitRegionsResponse>, Status> {
        let Some(cluster) = self.host.cluster() else {
            return Ok(Response::new(p::SplitRegionsResponse {
                header: Some(self.unavailable_cluster()),
                ..Default::default()
            }));
        };
        let r = request.into_inner();
        let cancel = self.cancel.child_token();
        let _guard = cancel.clone().drop_guard();
        let result = tokio::select! {biased;_=self.cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=cluster.split_regions(SplitRequest {keys:r.split_keys,retries:r.retry_limit as i64,cancellation:cancel})=>result.map_err(Status::unknown)?};
        Ok(Response::new(p::SplitRegionsResponse {
            header: Some(self.header()),
            finished_percentage: result.percentage,
            regions_id: result.ids,
        }))
    }
    async fn scatter_regions(
        &self,
        request: Request<p::ScatterRegionsRequest>,
    ) -> Result<Response<p::ScatterRegionsResponse>, Status> {
        let Some(cluster) = self.host.cluster() else {
            return Ok(Response::new(p::ScatterRegionsResponse {
                header: Some(self.unavailable_cluster()),
                ..Default::default()
            }));
        };
        let r = request.into_inner();
        let result = tokio::select! {biased;_=self.cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=cluster.scatter(ScatterRequest {batch:true,range:None,ids:r.regions_id,single:0,fallback:None,group:r.group,retries:if (r.retry_limit as i64)<0 {-1} else {r.retry_limit.min(30) as i32},skip_store_limit:r.skip_store_limit})=>result};
        let response=match result {
            Ok(result)=>p::ScatterRegionsResponse {header:Some(self.header()),finished_percentage:result.percentage,failed_regions_id:result.failed},
            Err(error)=>p::ScatterRegionsResponse {header:Some(self.error(p::ErrorType::Unknown,match error {ScatterError::Other(message)=>message,ScatterError::Missing=>"region not found".into(),ScatterError::Rejected=>"operator canceled because cannot add an operator to the execute queue".into()})),..Default::default()},
        };
        Ok(Response::new(response))
    }
    async fn get_operator(
        &self,
        request: Request<p::GetOperatorRequest>,
    ) -> Result<Response<p::GetOperatorResponse>, Status> {
        let Some(cluster) = self.host.cluster() else {
            return Ok(Response::new(p::GetOperatorResponse {
                header: Some(self.unavailable_cluster()),
                ..Default::default()
            }));
        };
        let id = request.into_inner().region_id;
        let operator = tokio::select! {biased;_=self.cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=cluster.operator(id)=>result.map_err(Status::unknown)?};
        Ok(Response::new(match operator {
            Some(operator) => p::GetOperatorResponse {
                header: Some(self.header()),
                region_id: id,
                desc: operator.desc,
                kind: operator.kind,
                status: operator_status(operator.status),
            },
            None => p::GetOperatorResponse {
                header: Some(self.error(p::ErrorType::Unknown, "region not found")),
                ..Default::default()
            },
        }))
    }
    async fn ask_batch_split(
        &self,
        request: Request<p::AskBatchSplitRequest>,
    ) -> Result<Response<p::AskBatchSplitResponse>, Status> {
        let Some(cluster) = self.host.cluster() else {
            return Ok(Response::new(p::AskBatchSplitResponse {
                header: Some(self.unavailable_cluster()),
                ..Default::default()
            }));
        };
        let r = request.into_inner();
        let Some(region) = r.region else {
            return Ok(Response::new(p::AskBatchSplitResponse {
                header: Some(self.error(p::ErrorType::Unknown, "missing region for split")),
                ..Default::default()
            }));
        };
        let epoch_present = region.region_epoch.is_some();
        let result = tokio::select! {biased;_=self.cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=cluster.split_ids(astersql_pd_grpcutil::core_region_adapter::region_from_proto(Some(region)).unwrap(),r.split_count,true,r.reason)=>result};
        let response = match result {
            Ok(ids) => p::AskBatchSplitResponse {
                header: Some(self.header()),
                ids: ids
                    .into_iter()
                    .map(
                        |(new_region_id, new_peer_ids)| astersql_pd_proto::pdpb::SplitId {
                            new_region_id,
                            new_peer_ids,
                        },
                    )
                    .collect(),
            },
            Err(SplitError::Other(message)) => return Err(Status::unknown(message)),
            Err(SplitError::Forwarded(status)) => return Err(status),
            Err(error) => p::AskBatchSplitResponse {
                header: Some(self.error(
                    p::ErrorType::Unknown,
                    astersql_pd_server::grpc_ask_split::error_message(error, epoch_present),
                )),
                ..Default::default()
            },
        };
        Ok(Response::new(response))
    }
    async fn region_heartbeat(
        &self,
        request: Request<tonic::Streaming<p::RegionHeartbeatRequest>>,
    ) -> Result<Response<tonic::codegen::BoxStream<p::RegionHeartbeatResponse>>, Status> {
        Ok(Response::new(self.heartbeat_stream(request.into_inner())))
    }
    async fn region_buckets(
        &self,
        request: Request<tonic::Streaming<p::RegionBucketsRequest>>,
    ) -> Result<Response<tonic::codegen::BoxStream<p::RegionBucketsResponse>>, Status> {
        Ok(Response::new(self.buckets_stream(request.into_inner())))
    }
}
