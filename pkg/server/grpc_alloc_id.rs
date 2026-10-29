// Copyright 2026 AsterSQL.
//! Classic AllocID count normalization and protocol error mapping.
use crate::PdServices;
use astersql_pd_proto::pdpb;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

pub(crate) async fn handle(
    services: &dyn PdServices,
    cancel: &CancellationToken,
    request: Request<pdpb::AllocIdRequest>,
) -> Result<Response<pdpb::AllocIdResponse>, Status> {
    let request = request.into_inner();
    crate::request_role::validate(services, cancel, request.header.as_ref()).await?;
    let result = tokio::select! { biased;
        _ = cancel.cancelled() => return Err(Status::unavailable("server not started")),
        result = services.alloc_ids(request.count.max(1)) => result,
    };
    let mut response = pdpb::AllocIdResponse::default();
    match result {
        Ok((id, count)) => {
            response.header = Some(pdpb::ResponseHeader {
                cluster_id: services.identity().cluster_id,
                ..Default::default()
            });
            response.id = id;
            response.count = if count > 1 { count } else { 0 };
        }
        Err(message) => {
            response.header = Some(pdpb::ResponseHeader {
                error: Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                }),
                ..Default::default()
            });
        }
    }
    Ok(Response::new(response))
}
