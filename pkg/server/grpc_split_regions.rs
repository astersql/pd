// Copyright 2026 AsterSQL.
//! Split requests use the host's canonical splitter and scheduling controller.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};

/// Request parameters, without transport types.
pub struct SplitRequest {
    pub keys: Vec<Vec<u8>>,
    pub retries: i64,
    pub cancellation: tokio_util::sync::CancellationToken,
}
/// Only observed region boundaries count as successful splits.
pub struct SplitResult {
    pub percentage: u64,
    pub ids: Vec<u64>,
}
impl Grpc {
    pub(crate) async fn split_regions_request(
        &self,
        r: pdpb::SplitRegionsRequest,
    ) -> Result<Response<pdpb::SplitRegionsResponse>, Status> {
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::SplitRegionsResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        self.check(r.header.as_ref()).await?;
        let cancellation = tokio_util::sync::CancellationToken::new();
        let _guard = cancellation.clone().drop_guard();
        let result = tokio::select! { biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=self.services.split_regions(SplitRequest {
                keys:r.split_keys,
                // Go converts uint64 to native int without clamping retries.
                retries: r.retry_limit as i64,
                cancellation,
            })=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::SplitRegionsResponse {
            header: self.header(),
            finished_percentage: result.percentage,
            regions_id: result.ids,
        }))
    }
}
