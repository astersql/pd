// Copyright 2026 AsterSQL.
//! Point lookups over host-owned canonical leader or synchronized follower views.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Request, Status};
impl Grpc {
    pub(crate) async fn region_request(
        &self,
        request: Request<pdpb::GetRegionRequest>,
        previous: bool,
    ) -> Result<pdpb::GetRegionResponse, Status> {
        let allow = request.metadata().contains_key("pd-allow-follower-handle");
        let r = request.into_inner();
        let result = self
            .lookup_region(
                r.header.as_ref(),
                Some(&r.region_key),
                0,
                previous,
                r.need_buckets,
                allow,
            )
            .await;
        if let (Some(metrics), Ok(response)) = (self.services.service_metrics(), &result) {
            metrics.region_result(
                if previous {
                    "GetPrevRegion"
                } else {
                    "GetRegion"
                },
                response.header.as_ref().is_some_and(|h| h.error.is_some()),
            );
        }
        result
    }
    pub(crate) async fn lookup_region(
        &self,
        header: Option<&pdpb::RequestHeader>,
        key: Option<&[u8]>,
        id: u64,
        previous: bool,
        need_buckets: bool,
        allow: bool,
    ) -> Result<pdpb::GetRegionResponse, Status> {
        let operation = async {
            let follower = match self.services.healthy().await {
                Ok(()) => false,
                Err(e) if e == "not leader" => {
                    if !allow {
                        return Err(Status::unavailable(
                            "not leader and follower handling not allowed",
                        ));
                    }
                    true
                }
                Err(e) => return Err(Status::unavailable(e)),
            };
            let expected = self.services.identity().cluster_id;
            let received = header.map_or(0, |h| h.cluster_id);
            if expected != received {
                return Err(Status::failed_precondition(format!(
                    "mismatch cluster id, need {expected} but got {received}"
                )));
            }
            if !follower && let Some(header) = self.bootstrap_header().await? {
                return Ok(pdpb::GetRegionResponse {
                    header: Some(header),
                    ..Default::default()
                });
            }
            let service = if follower {
                self.services.follower_read_service()
            } else {
                self.services.cluster_read_service()
            };
            let region = if let Some(service) = service {
                if let Some(key) = key {
                    service.region_by_key(key, previous).await
                } else {
                    service.region(id).await
                }
                .map_err(Status::unavailable)?
            } else if follower {
                None
            } else {
                return Err(Status::unimplemented("cluster reads are not configured"));
            };
            let mut header = self.header().unwrap();
            if follower && region.is_none() {
                header.error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::RegionNotFound as i32,
                    message: "region not found".into(),
                });
            }
            let response = astersql_pd_grpcutil::cluster_read::point(
                header,
                region.as_deref(),
                need_buckets && !follower && self.services.region_buckets_enabled(),
            );
            Ok(response)
        };
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(Status::unavailable("server not started")), result=operation=>result}
    }
}
