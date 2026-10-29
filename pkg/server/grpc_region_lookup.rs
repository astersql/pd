// Copyright 2026 AsterSQL.
//! Point lookups over host-owned canonical leader or synchronized follower views.
use crate::{
    core_region_adapter::{peer_to_proto, region_to_proto},
    grpc::Grpc,
};
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
        self.lookup_region(
            r.header.as_ref(),
            Some(&r.region_key),
            0,
            previous,
            r.need_buckets,
            allow,
        )
        .await
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
            let mut response = pdpb::GetRegionResponse {
                header: Some(header),
                ..Default::default()
            };
            if let Some(r) = region {
                response.region = Some(region_to_proto(r.meta().clone()));
                response.leader = r.leader().cloned().map(peer_to_proto);
                response.down_peers = r
                    .down_peers()
                    .iter()
                    .map(|p| pdpb::PeerStats {
                        peer: Some(peer_to_proto(p.peer.clone())),
                        down_seconds: p.down_seconds,
                    })
                    .collect();
                response.pending_peers = r
                    .pending_peers()
                    .iter()
                    .cloned()
                    .map(peer_to_proto)
                    .collect();
                if need_buckets && !follower && self.services.region_buckets_enabled() {
                    response.buckets = r.buckets().map(|b| astersql_pd_proto::metapb::Buckets {
                        region_id: b.region_id,
                        version: b.version,
                        keys: b.keys,
                        period_in_ms: b.period_in_ms,
                        stats: b.stats.map(|s| astersql_pd_proto::metapb::BucketStats {
                            read_bytes: s.read_bytes,
                            read_keys: s.read_keys,
                            read_qps: s.read_qps,
                            write_bytes: s.write_bytes,
                            write_keys: s.write_keys,
                            write_qps: s.write_qps,
                        }),
                    });
                }
            }
            Ok(response)
        };
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(Status::unavailable("server not started")), result=operation=>result}
    }
}
