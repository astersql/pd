// Copyright 2026 AsterSQL.
//! Range scan transport over the canonical host cache.
use crate::{
    core_region_adapter::{peer_to_proto, region_to_proto},
    grpc::Grpc,
};
use astersql_pd_core::{BatchScanRegionsOptions, KeyRange};
use astersql_pd_proto::pdpb;
use tonic::{Request, Status};
impl Grpc {
    async fn scan_view(
        &self,
        header: Option<&pdpb::RequestHeader>,
        allow: bool,
    ) -> Result<(bool, Option<pdpb::ResponseHeader>), Status> {
        let follower = match self.services.healthy().await {
            Ok(()) => false,
            Err(e) if e == "not leader" && allow => true,
            Err(e) if e == "not leader" => {
                return Err(Status::unavailable(
                    "not leader and follower handling not allowed",
                ));
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
        Ok((
            follower,
            if follower {
                None
            } else {
                self.bootstrap_header().await?
            },
        ))
    }
    pub(crate) async fn scan_regions_request(
        &self,
        request: Request<pdpb::ScanRegionsRequest>,
    ) -> Result<pdpb::ScanRegionsResponse, Status> {
        let allow = request.metadata().contains_key("pd-allow-follower-handle");
        let r = request.into_inner();
        let operation = async {
            let (follower, boot) = self.scan_view(r.header.as_ref(), allow).await?;
            if let Some(header) = boot {
                return Ok(pdpb::ScanRegionsResponse {
                    header: Some(header),
                    ..Default::default()
                });
            }
            let service = if follower {
                self.services.follower_read_service()
            } else {
                self.services.cluster_read_service()
            };
            let regions = if let Some(s) = service {
                s.scan_regions(&r.start_key, &r.end_key, r.limit as isize)
                    .await
                    .map_err(Status::unavailable)?
            } else if follower {
                vec![]
            } else {
                return Err(Status::unimplemented("cluster reads are not configured"));
            };
            let mut response = pdpb::ScanRegionsResponse {
                header: self.header(),
                ..Default::default()
            };
            for r in regions {
                let r = wire_region(&r);
                response.region_metas.push(r.region.clone().unwrap());
                response.leaders.push(r.leader.clone().unwrap());
                response.regions.push(r);
            }
            if follower && response.regions.is_empty() {
                response.header.as_mut().unwrap().error = Some(not_found());
            }
            Ok(response)
        };
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(Status::unavailable("server not started")), result=operation=>result}
    }
    pub(crate) async fn batch_scan_regions_request(
        &self,
        request: Request<pdpb::BatchScanRegionsRequest>,
    ) -> Result<pdpb::BatchScanRegionsResponse, Status> {
        let allow = request.metadata().contains_key("pd-allow-follower-handle");
        let r = request.into_inner();
        let operation = async {
            let (follower, boot) = self.scan_view(r.header.as_ref(), allow).await?;
            let mut response = pdpb::BatchScanRegionsResponse {
                header: boot.or_else(|| self.header()),
                ..Default::default()
            };
            if response.header.as_ref().unwrap().error.is_some() {
                return Ok(response);
            }
            for (i, range) in r.ranges.iter().enumerate() {
                let error = if i > 0 && range.start_key < r.ranges[i - 1].end_key {
                    Some("invalid key range, ranges overlapped")
                } else if !range.end_key.is_empty() && range.start_key > range.end_key {
                    Some("invalid key range, start key > end key")
                } else {
                    None
                };
                if let Some(message) = error {
                    response.header.as_mut().unwrap().error = Some(pdpb::Error {
                        r#type: pdpb::ErrorType::Unknown as i32,
                        message: message.into(),
                    });
                    return Ok(response);
                }
            }
            let ranges: Vec<_> = r
                .ranges
                .iter()
                .map(|r| KeyRange {
                    start_key: r.start_key.clone(),
                    end_key: r.end_key.clone(),
                })
                .collect();
            let service = if follower {
                self.services.follower_read_service()
            } else {
                self.services.cluster_read_service()
            };
            let result = if let Some(s) = service {
                s.batch_scan_regions(
                    &ranges,
                    BatchScanRegionsOptions {
                        limit: r.limit as isize,
                        output_must_contain_all_key_range: r.contain_all_key_range,
                    },
                )
                .await
                .map_err(Status::unavailable)?
            } else if follower {
                Ok(vec![])
            } else {
                return Err(Status::unimplemented("cluster reads are not configured"));
            };
            match result {
                Ok(regions) => {
                    for region in regions {
                        let mut wire = wire_region(&region);
                        if r.need_buckets && !follower {
                            wire.buckets =
                                region
                                    .buckets()
                                    .map(|b| astersql_pd_proto::metapb::Buckets {
                                        region_id: b.region_id,
                                        version: b.version,
                                        keys: b.keys,
                                        period_in_ms: b.period_in_ms,
                                        stats: b.stats.map(|s| {
                                            astersql_pd_proto::metapb::BucketStats {
                                                read_bytes: s.read_bytes,
                                                read_keys: s.read_keys,
                                                read_qps: s.read_qps,
                                                write_bytes: s.write_bytes,
                                                write_keys: s.write_keys,
                                                write_qps: s.write_qps,
                                            }
                                        }),
                                    });
                        }
                        response.regions.push(wire);
                    }
                    if follower && response.regions.is_empty() {
                        response.header.as_mut().unwrap().error = Some(not_found());
                    }
                }
                Err(e) => {
                    response.header.as_mut().unwrap().error = Some(pdpb::Error {
                        r#type: pdpb::ErrorType::RegionsNotContainAllKeyRange as i32,
                        message: e.to_string(),
                    })
                }
            }
            Ok(response)
        };
        tokio::select! {biased; _=self.cancel.cancelled()=>Err(Status::unavailable("server not started")), result=operation=>result}
    }
}
fn not_found() -> pdpb::Error {
    pdpb::Error {
        r#type: pdpb::ErrorType::RegionNotFound as i32,
        message: "region not found".into(),
    }
}
fn wire_region(r: &astersql_pd_core::RegionInfo) -> pdpb::Region {
    pdpb::Region {
        region: Some(region_to_proto(r.meta().clone())),
        leader: Some(r.leader().cloned().map(peer_to_proto).unwrap_or_default()),
        down_peers: r
            .down_peers()
            .iter()
            .map(|p| pdpb::PeerStats {
                peer: Some(peer_to_proto(p.peer.clone())),
                down_seconds: p.down_seconds,
            })
            .collect(),
        pending_peers: r
            .pending_peers()
            .iter()
            .cloned()
            .map(peer_to_proto)
            .collect(),
        ..Default::default()
    }
}
