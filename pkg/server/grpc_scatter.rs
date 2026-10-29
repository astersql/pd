// Copyright 2026 AsterSQL.
//! Classic scatter and operator queries over the host's canonical coordinator.
use crate::grpc::Grpc;
use astersql_pd_core::RegionInfo;
use astersql_pd_proto::pdpb;
use std::sync::Arc;
use tonic::{Response, Status};

/// Transport-independent request forwarded to the term-owned scheduling task.
pub struct ScatterRequest {
    pub ids: Vec<u64>,
    pub single: u64,
    pub fallback: Option<Arc<RegionInfo>>,
    pub group: String,
    pub retries: i32,
    pub skip_store_limit: bool,
}
/// Successful planning result; admission failures retain their region IDs.
pub struct ScatterResult {
    pub percentage: u64,
    pub failed: Vec<u64>,
}
/// Single-region protocol errors differ from batch transport errors.
pub enum ScatterError {
    Missing,
    Rejected,
    Other(String),
}
/// Frozen view of the canonical active or retained operator.
pub struct OperatorResult {
    pub desc: Vec<u8>,
    pub kind: Vec<u8>,
    pub status: u32,
}
impl Grpc {
    pub(crate) async fn scatter_request(
        &self,
        r: pdpb::ScatterRegionRequest,
    ) -> Result<Response<pdpb::ScatterRegionResponse>, Status> {
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::ScatterRegionResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        self.check(r.header.as_ref()).await?;
        let fallback = crate::core_region_adapter::region_from_proto(r.region).map(|meta| {
            Arc::new(RegionInfo::new(
                meta,
                r.leader.map(crate::core_region_adapter::peer_from_proto),
                vec![],
                vec![],
            ))
        });
        let request = ScatterRequest {
            ids: r.regions_id,
            single: r.region_id,
            fallback,
            group: r.group,
            retries: if (r.retry_limit as i64) < 0 {
                -1
            } else {
                r.retry_limit.min(30) as i32
            },
            skip_store_limit: r.skip_store_limit,
        };
        let result = tokio::select! { biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=self.services.scatter(request)=>result,
        };
        let mut response = pdpb::ScatterRegionResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok(result) => {
                response.finished_percentage = result.percentage;
                response.failed_regions_id = result.failed;
            }
            Err(ScatterError::Other(error)) => return Err(Status::unknown(error)),
            Err(error) => {
                response.header.as_mut().unwrap().error = Some(match error {
                    ScatterError::Missing => pdpb::Error {
                        r#type: pdpb::ErrorType::RegionNotFound as i32,
                        message: "region %d not found".into(),
                    },
                    _ => pdpb::Error {
                        r#type: pdpb::ErrorType::Unknown as i32,
                        message:
                            "operator canceled because cannot add an operator to the execute queue"
                                .into(),
                    },
                })
            }
        }
        Ok(Response::new(response))
    }
    pub(crate) async fn operator_request(
        &self,
        r: pdpb::GetOperatorRequest,
    ) -> Result<Response<pdpb::GetOperatorResponse>, Status> {
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetOperatorResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        self.check(r.header.as_ref()).await?;
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=self.services.operator(r.region_id)=>result.map_err(Status::unknown)?,
        };
        let mut response = pdpb::GetOperatorResponse {
            header: self.header(),
            ..Default::default()
        };
        if let Some(result) = result {
            response.region_id = r.region_id;
            response.desc = result.desc;
            response.kind = result.kind;
            response.status = match result.status {
                1 => 4,
                2 => 0,
                3 => 2,
                4 => 3,
                5 | 6 => 1,
                _ => 5,
            };
        } else {
            response.header.as_mut().unwrap().error = Some(pdpb::Error {
                r#type: pdpb::ErrorType::RegionNotFound as i32,
                message: "Not Found".into(),
            });
        }
        Ok(Response::new(response))
    }
}
