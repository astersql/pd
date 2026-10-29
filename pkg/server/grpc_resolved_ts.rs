// Copyright 2026 AsterSQL.
//! Per-store reports update volatile state; the existing worker persists the cluster minimum.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn report_min_resolved_ts_request(
        &self,
        request: pdpb::ReportMinResolvedTsRequest,
    ) -> Result<Response<pdpb::ReportMinResolvedTsResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::ReportMinResolvedTsResponse {
                header: Some(header),
            }));
        }
        tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = self.services.report_min_resolved_ts(request.store_id, request.min_resolved_ts) =>
                result.map_err(Status::unknown)?,
        }
        Ok(Response::new(pdpb::ReportMinResolvedTsResponse {
            header: self.header(),
        }))
    }
}
