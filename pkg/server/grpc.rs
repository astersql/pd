// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_proto::pdpb::{self, pd_server::Pd};
use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

pub struct Grpc {
    pub services: Arc<dyn PdServices>,
    pub cancel: CancellationToken,
}
impl Grpc {
    fn header(&self) -> Option<pdpb::ResponseHeader> {
        Some(pdpb::ResponseHeader {
            cluster_id: self.services.identity().cluster_id,
            ..Default::default()
        })
    }
    fn check(&self, header: Option<&pdpb::RequestHeader>) -> Result<(), Status> {
        if header.map(|h| h.cluster_id).unwrap_or(0) != self.services.identity().cluster_id {
            return Err(Status::failed_precondition(format!(
                "mismatch cluster id, need {} but got {}",
                self.services.identity().cluster_id,
                header.map(|h| h.cluster_id).unwrap_or(0)
            )));
        }
        Ok(())
    }
}
pub(crate) fn member(id: &Identity) -> pdpb::Member {
    pdpb::Member {
        member_id: id.member_id,
        name: id.name.clone(),
        client_urls: vec![id.client_url.clone()],
        peer_urls: vec![id.peer_url.clone()],
        ..Default::default()
    }
}
#[tonic::async_trait]
impl Pd for Grpc {
    async fn store_heartbeat(
        &self,
        request: Request<pdpb::StoreHeartbeatRequest>,
    ) -> Result<Response<pdpb::StoreHeartbeatResponse>, Status> {
        use crate::store_heartbeat_adapter::StoreHeartbeatError;
        let request = request.into_inner();
        self.check(request.header.as_ref())?;
        self.services.healthy().await.map_err(Status::unavailable)?;
        // Do not include a debug dump of the wire payload in transport errors.
        let stats = request
            .stats
            .ok_or_else(|| Status::unknown("invalid store heartbeat command"))?;
        let service = self.services.store_heartbeat_service().ok_or_else(|| {
            Status::unimplemented("store heartbeat state service is not configured")
        })?;
        // These fields belong to recovery/replication controllers outside this
        // state adapter. In particular a recovery report must bypass statistics.
        if request.store_report.is_some() || request.dr_autosync_status.is_some() {
            return Err(Status::unimplemented(
                "store recovery and DR status are unsupported",
            ));
        }
        let stats = crate::core_store_adapter::store_stats_from_proto(Some(stats)).unwrap();
        let result = service.store_heartbeat(stats).await;
        let mut header = self.header().unwrap();
        let state = match result {
            Ok(state) => match state {
                astersql_pd_core::NodeState::Preparing => 0,
                astersql_pd_core::NodeState::Serving => 1,
                astersql_pd_core::NodeState::Removing => 2,
                astersql_pd_core::NodeState::Removed => 3,
                astersql_pd_core::NodeState::Unknown(value) => value,
            },
            Err(error) => {
                let (kind, message) = match error {
                    StoreHeartbeatError::NotBootstrapped => (
                        pdpb::ErrorType::NotBootstrapped,
                        "cluster is not bootstrapped".into(),
                    ),
                    StoreHeartbeatError::StoreNotFound(id) => {
                        (pdpb::ErrorType::Unknown, format!("store {id} not found"))
                    }
                    StoreHeartbeatError::Removed => {
                        (pdpb::ErrorType::StoreTombstone, "store is tombstone".into())
                    }
                    StoreHeartbeatError::Other(message) => (pdpb::ErrorType::Unknown, message),
                };
                header.error = Some(pdpb::Error {
                    r#type: kind as i32,
                    message,
                });
                0
            }
        };
        Ok(Response::new(pdpb::StoreHeartbeatResponse {
            header: Some(header),
            state,
            ..Default::default()
        }))
    }
    async fn get_members(
        &self,
        _: Request<pdpb::GetMembersRequest>,
    ) -> Result<Response<pdpb::GetMembersResponse>, Status> {
        self.services.healthy().await.map_err(Status::unavailable)?;
        let member = member(self.services.identity());
        Ok(Response::new(pdpb::GetMembersResponse {
            header: self.header(),
            members: vec![member.clone()],
            leader: Some(member),
            ..Default::default()
        }))
    }
    async fn alloc_id(
        &self,
        req: Request<pdpb::AllocIdRequest>,
    ) -> Result<Response<pdpb::AllocIdResponse>, Status> {
        let req = req.into_inner();
        self.check(req.header.as_ref())?;
        if req.count > 1 {
            return Err(Status::unimplemented("batch ID allocation is unsupported"));
        }
        let id = self
            .services
            .alloc_id()
            .await
            .map_err(Status::unavailable)?;
        Ok(Response::new(pdpb::AllocIdResponse {
            header: self.header(),
            id,
            ..Default::default()
        }))
    }
    async fn tso(
        &self,
        req: Request<tonic::Streaming<pdpb::TsoRequest>>,
    ) -> Result<
        Response<Pin<Box<dyn Stream<Item = Result<pdpb::TsoResponse, Status>> + Send + 'static>>>,
        Status,
    > {
        let input = req.into_inner();
        let services = self.services.clone();
        let cancel = self.cancel.clone();
        // The response stream owns all work; dropping it cannot detach a request worker.
        let output = futures_util::stream::unfold(
            (input, services, cancel, false),
            |(mut input, services, cancel, terminal)| async move {
                if terminal {
                    return None;
                }
                let request = tokio::select! { biased;
                    _ = cancel.cancelled() => return None,
                    request = input.message() => request,
                };
                let result = match request {
                    Ok(Some(req)) => {
                        if req.header.as_ref().map(|h| h.cluster_id).unwrap_or(0)
                            != services.identity().cluster_id
                        {
                            Err(Status::failed_precondition(format!(
                                "mismatch cluster id, need {} but got {}",
                                services.identity().cluster_id,
                                req.header.as_ref().map(|h| h.cluster_id).unwrap_or(0)
                            )))
                        } else if !req.dc_location.is_empty() && req.dc_location != "global" {
                            Err(Status::unimplemented("local TSO is unsupported"))
                        } else if req.count == 0 || req.count >= astersql_pd_tso::MAX_LOGICAL {
                            Err(Status::unknown(
                                "[PD:tso:ErrGenerateTimestamp]generate timestamp failed, tso count should be positive",
                            ))
                        } else {
                            services
                                .timestamp(req.count)
                                .await
                                .map(|ts| pdpb::TsoResponse {
                                    header: Some(pdpb::ResponseHeader {
                                        cluster_id: services.identity().cluster_id,
                                        ..Default::default()
                                    }),
                                    count: req.count,
                                    timestamp: Some(pdpb::Timestamp {
                                        physical: ts.physical as i64,
                                        logical: ts.logical as i64,
                                        suffix_bits: 0,
                                    }),
                                })
                                .map_err(Status::unavailable)
                        }
                    }
                    Ok(None) => return None,
                    Err(status) => Err(status),
                };
                let terminal = result.is_err();
                Some((result, (input, services, cancel, terminal)))
            },
        );
        Ok(Response::new(Box::pin(output)))
    }
}
