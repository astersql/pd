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
