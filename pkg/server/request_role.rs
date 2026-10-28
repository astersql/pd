// Copyright 2026 AsterSQL.
//! Shared admission for requests that require a serving leader.
use crate::PdServices;
use astersql_pd_proto::pdpb;
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// Match validateRoleInRequest: stopped/leader admission precedes cluster identity.
/// The host validates its current lease; transport never caches a leader decision.
/// Streaming heartbeat callers deliberately check bootstrap before calling this,
/// matching the distinct ordering of those Go methods.
pub(crate) async fn validate(
    services: &dyn PdServices,
    cancel: &CancellationToken,
    header: Option<&pdpb::RequestHeader>,
) -> Result<(), Status> {
    if cancel.is_cancelled() {
        return Err(Status::unavailable("server not started"));
    }
    tokio::select! { biased;
        _ = cancel.cancelled() => return Err(Status::unavailable("server not started")),
        result = services.healthy() => result.map_err(Status::unavailable)?,
    }
    let expected = services.identity().cluster_id;
    let received = header.map(|header| header.cluster_id).unwrap_or(0);
    if received != expected {
        return Err(Status::failed_precondition(format!(
            "mismatch cluster id, need {expected} but got {received}"
        )));
    }
    Ok(())
}

/// Protocol-level absence of bootstrap is a successful RPC with an error header.
pub(crate) fn not_bootstrapped(services: &dyn PdServices) -> pdpb::ResponseHeader {
    pdpb::ResponseHeader {
        cluster_id: services.identity().cluster_id,
        error: Some(pdpb::Error {
            r#type: pdpb::ErrorType::NotBootstrapped as i32,
            message: "cluster is not bootstrapped".into(),
        }),
    }
}
