// Copyright 2026 AsterSQL.
//! Compose the canonical split observer and scatter coordinator in Go order.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn split_scatter_request(
        &self,
        r: pdpb::SplitAndScatterRegionsRequest,
    ) -> Result<Response<pdpb::SplitAndScatterRegionsResponse>, Status> {
        // Unlike SplitRegions, Go's combined RPC runs unary middleware before
        // looking up the bootstrapped raft cluster.
        self.check(r.header.as_ref()).await?;
        let split = self
            .split_regions_request(pdpb::SplitRegionsRequest {
                header: r.header.clone(),
                split_keys: r.split_keys,
                retry_limit: r.retry_limit,
            })
            .await?
            .into_inner();
        if split.header.as_ref().is_some_and(|h| h.error.is_some()) {
            return Ok(Response::new(pdpb::SplitAndScatterRegionsResponse {
                header: split.header,
                ..Default::default()
            }));
        }
        // The standalone RPC treats empty IDs as a single-region request. The
        // combined RPC always calls Go's batch path, including its empty error.
        if split.regions_id.is_empty() {
            return Err(Status::unknown("[PD:scatter:ErrEmptyRegion]empty region"));
        }
        let scatter = self
            .scatter_request(pdpb::ScatterRegionRequest {
                header: r.header,
                regions_id: split.regions_id.clone(),
                group: r.group,
                retry_limit: r.retry_limit,
                skip_store_limit: false,
                ..Default::default()
            })
            .await?
            .into_inner();
        Ok(Response::new(pdpb::SplitAndScatterRegionsResponse {
            header: scatter.header,
            regions_id: split.regions_id,
            split_finished_percentage: split.finished_percentage,
            scatter_finished_percentage: scatter.finished_percentage,
        }))
    }
}
