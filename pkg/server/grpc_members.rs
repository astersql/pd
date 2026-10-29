// Copyright 2026 AsterSQL.
//! Classic PD discovery, independent of local leadership and bootstrap admission.
use crate::PdServices;
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, Read};
use prost::Message;
use tokio_util::sync::CancellationToken;
use tonic::Status;

pub(crate) async fn members(
    services: &dyn PdServices,
    cancel: &CancellationToken,
) -> Result<pdpb::GetMembersResponse, Status> {
    if cancel.is_cancelled() {
        return Err(Status::unavailable("server not started"));
    }
    let cluster_id = services.identity().cluster_id;
    let result = async {
        let store = services
            .discovery_store()
            .ok_or("member discovery is not configured")?;
        let (members, revision) = registry_members(store.as_ref(), cluster_id)
            .await
            .map_err(|error| error.message().to_owned())?;
        let key = format!("/pd/{cluster_id}/leader");
        let mut read = Read::range(key.as_bytes(), b"", Consistency::Linearizable);
        // Pin the leader to the member snapshot so a concurrent election cannot
        // return a leader absent from a stale list. No local term is consulted.
        read.revision = Some(revision);
        let leader = store
            .read(read)
            .await
            .map_err(|e| e.to_string())?
            .entries
            .into_iter()
            .next()
            .map(|e| pdpb::Member::decode(e.value.as_slice()).map_err(|e| e.to_string()))
            .transpose()?;
        let pd_leader = leader.and_then(|leader| {
            members
                .iter()
                .find(|m| m.member_id == leader.member_id)
                .cloned()
        });
        let etcd_leader = store
            .consensus_leader_id()
            .await
            .map_err(|e| e.to_string())?
            .and_then(|id| members.iter().find(|m| m.member_id == id).cloned());
        Ok::<_, String>((members, pd_leader, etcd_leader))
    };
    let result = tokio::select! { biased;
        _ = cancel.cancelled() => return Err(Status::unavailable("server not started")),
        result = result => result,
    };
    let mut response = pdpb::GetMembersResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id,
            error: None,
        }),
        ..Default::default()
    };
    match result {
        Ok((members, leader, etcd_leader)) => {
            response.members = members;
            response.leader = leader;
            response.etcd_leader = etcd_leader;
        }
        Err(message) => {
            response.header.as_mut().unwrap().error = Some(pdpb::Error {
                r#type: pdpb::ErrorType::Unknown as i32,
                message,
            })
        }
    }
    Ok(response)
}

/// Shared registry decoding for HTTP health and gRPC membership. No leadership
/// or backend consensus diagnostics are needed to inspect member liveness.
pub(crate) async fn registry_members(
    store: &dyn astersql_pd_storage::MetaStore,
    cluster_id: u64,
) -> Result<(Vec<pdpb::Member>, i64), Status> {
    let prefix = format!("/pd/{cluster_id}/rust/members/");
    let snapshot = store
        .read(Read::prefix(prefix.as_bytes(), Consistency::Linearizable))
        .await
        .map_err(|error| match error {
            astersql_pd_storage::StoreError::Unavailable
            | astersql_pd_storage::StoreError::Cancelled => Status::unavailable(error.to_string()),
            error => Status::internal(error.to_string()),
        })?;
    let mut members = Vec::with_capacity(snapshot.entries.len());
    for entry in snapshot.entries {
        let id = std::str::from_utf8(&entry.key)
            .ok()
            .and_then(|key| key.strip_prefix(&prefix))
            .and_then(|key| key.parse::<u64>().ok())
            .filter(|id| *id != 0)
            .ok_or_else(|| Status::internal("invalid member registry key"))?;
        let member = pdpb::Member::decode(entry.value.as_slice())
            .map_err(|e| Status::internal(e.to_string()))?;
        if member.member_id != id {
            return Err(Status::internal("member registry identity mismatch"));
        }
        // Go's gRPC membership exposes consensus identity fields only; binary
        // publication metadata belongs to the HTTP member administration API.
        members.push(pdpb::Member {
            member_id: id,
            name: member.name,
            client_urls: member.client_urls,
            peer_urls: member.peer_urls,
            ..Default::default()
        });
    }
    Ok((members, snapshot.revision))
}
