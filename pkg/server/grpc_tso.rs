// Copyright 2026 AsterSQL.
//! Classic minimum timestamps use the same host-owned oracle as the Tso stream.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn get_min_ts_request(
        &self,
        request: pdpb::GetMinTsRequest,
    ) -> Result<Response<pdpb::GetMinTsResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        let started = std::time::Instant::now();
        let timestamp = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = self.services.timestamp(1) => result,
        };
        if timestamp.is_ok()
            && let Some(metrics) = self.services.service_metrics()
        {
            metrics
                .tso_handling
                .observe(started.elapsed().as_secs_f64());
        }
        // Go's classic branch ignores internalErr after admission. Preserve its
        // zero timestamp on generation failure; MCS error headers are separate.
        let timestamp = timestamp
            .map(|ts| pdpb::Timestamp {
                physical: ts.physical as i64,
                logical: i64::from(ts.logical),
                suffix_bits: 0,
            })
            .unwrap_or_default();
        Ok(Response::new(pdpb::GetMinTsResponse {
            header: self.header(),
            timestamp: Some(timestamp),
        }))
    }
}
