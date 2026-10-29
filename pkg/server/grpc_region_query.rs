// Copyright 2026 AsterSQL.
//! Ordered, demand-driven queries of the host's current region view.
use crate::{
    core_region_adapter::{peer_to_proto, region_to_proto},
    grpc::Grpc,
};
use astersql_pd_proto::pdpb;
use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
pub(crate) type QueryStream =
    Pin<Box<dyn Stream<Item = Result<pdpb::QueryRegionResponse, Status>> + Send + 'static>>;
impl Grpc {
    pub(crate) fn query_stream(
        &self,
        request: Request<tonic::Streaming<pdpb::QueryRegionRequest>>,
    ) -> Response<QueryStream> {
        let grpc = Grpc {
            services: self.services.clone(),
            cancel: self.cancel.clone(),
        };
        // No detached producer: dropping the response drops input and in-flight reads.
        let stream = futures_util::stream::unfold(
            (request.into_inner(), grpc, false),
            |(mut input, grpc, terminal)| async move {
                if terminal {
                    return None;
                }
                let result = tokio::select! {biased;
                    _=grpc.cancel.cancelled()=>Err(Status::unavailable("server not started")),
                    next=input.message()=>match next {Ok(Some(r))=>grpc.query_request(r).await,Ok(None)=>return None,Err(e)=>Err(e)}
                };
                let terminal = result.is_err();
                Some((result, (input, grpc, terminal)))
            },
        );
        Response::new(Box::pin(stream))
    }
    pub(crate) async fn query_request(
        &self,
        r: pdpb::QueryRegionRequest,
    ) -> Result<pdpb::QueryRegionResponse, Status> {
        let operation = async {
            let expected = self.services.identity().cluster_id;
            let received = r.header.as_ref().map_or(0, |h| h.cluster_id);
            if expected != received {
                return Err(Status::failed_precondition(format!(
                    "mismatch cluster id, need {expected} but got {received}"
                )));
            }
            let follower = match self.services.healthy().await {
                Ok(()) => false,
                Err(e) if e == "not leader" => true,
                Err(e) => return Err(Status::unavailable(e)),
            };
            if !follower && let Some(header) = self.bootstrap_header().await? {
                return Ok(pdpb::QueryRegionResponse {
                    header: Some(header),
                    ..Default::default()
                });
            }
            let service = if follower {
                self.services.follower_read_service()
            } else {
                self.services.cluster_read_service()
            };
            let result = if let Some(service) = service {
                service
                    .query_regions(
                        &r.keys,
                        &r.prev_keys,
                        &r.ids,
                        r.need_buckets && !follower && self.services.region_buckets_enabled(),
                    )
                    .await
                    .map_err(Status::unavailable)?
            } else if follower {
                None
            } else {
                return Err(Status::unimplemented("cluster reads are not configured"));
            };
            let mut response = pdpb::QueryRegionResponse {
                header: self.header(),
                ..Default::default()
            };
            let Some(result) = result else {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::RegionNotFound as i32,
                    message: "region not found".into(),
                });
                return Ok(response);
            };
            response.key_id_map = result.key_ids;
            response.prev_key_id_map = result.prev_key_ids;
            for (id, region) in result.regions_by_id {
                let wire = region
                    .map(|r| pdpb::RegionResponse {
                        region: Some(region_to_proto(r.region)),
                        leader: r.leader.map(peer_to_proto),
                        down_peers: r
                            .down_peers
                            .into_iter()
                            .map(|p| pdpb::PeerStats {
                                peer: Some(peer_to_proto(p.peer)),
                                down_seconds: p.down_seconds,
                            })
                            .collect(),
                        pending_peers: r.pending_peers.into_iter().map(peer_to_proto).collect(),
                        buckets: r.buckets.map(|b| astersql_pd_proto::metapb::Buckets {
                            region_id: b.region_id,
                            version: b.version,
                            keys: b.keys.clone(),
                            period_in_ms: b.period_in_ms,
                            stats: b.stats.as_ref().map(|s| {
                                astersql_pd_proto::metapb::BucketStats {
                                    read_bytes: s.read_bytes.clone(),
                                    read_keys: s.read_keys.clone(),
                                    read_qps: s.read_qps.clone(),
                                    write_bytes: s.write_bytes.clone(),
                                    write_keys: s.write_keys.clone(),
                                    write_qps: s.write_qps.clone(),
                                }
                            }),
                        }),
                    })
                    .unwrap_or_default();
                response.regions_by_id.insert(id, wire);
            }
            Ok(response)
        };
        tokio::select! {biased;_=self.cancel.cancelled()=>Err(Status::unavailable("server not started")),r=operation=>r}
    }
}
