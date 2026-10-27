// Copyright 2026 AsterSQL.
//! Scheduling commands cross the protobuf boundary only in this adapter.
use crate::core_region_adapter::{peer_to_proto, region_to_proto};
use astersql_pd_core::Peer;
use astersql_pd_proto::{metapb, pdpb, schedulingpb};
use astersql_pd_schedule_hbstream::{self as domain, HeartbeatCommand};

/// A nil repeated message cannot be encoded by Go's generated marshaler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireError {
    pub field: &'static str,
    pub index: usize,
}
impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "missing message at {}[{}]", self.field, self.index)
    }
}
impl std::error::Error for WireError {}
fn repeated<T, U>(
    values: &[Option<T>],
    field: &'static str,
    convert: impl Fn(&T) -> U,
) -> Result<Vec<U>, WireError> {
    values
        .iter()
        .enumerate()
        .map(|(index, v)| v.as_ref().map(&convert).ok_or(WireError { field, index }))
        .collect()
}
fn change(p: &domain::ChangePeer) -> pdpb::ChangePeer {
    pdpb::ChangePeer {
        peer: p.peer.clone().map(peer_to_proto),
        change_type: p.change_type.as_raw(),
    }
}

/// Convert a classic PD command, preserving all independently present actions.
pub fn to_pdpb(c: &HeartbeatCommand) -> Result<pdpb::RegionHeartbeatResponse, WireError> {
    let op = &c.operation;
    Ok(pdpb::RegionHeartbeatResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id: c.cluster_id,
            error: None,
        }),
        region_id: c.region_id,
        region_epoch: c.region_epoch.map(|e| metapb::RegionEpoch {
            conf_ver: e.conf_ver,
            version: e.version,
        }),
        target_peer: Some(peer_to_proto(c.target_peer.clone())),
        change_peer: op.change_peer.as_ref().map(change),
        transfer_leader: op
            .transfer_leader
            .as_ref()
            .map(|t| {
                Ok(pdpb::TransferLeader {
                    peer: t.peer.clone().map(peer_to_proto),
                    peers: repeated(&t.peers, "transfer_leader.peers", |p| {
                        peer_to_proto(p.clone())
                    })?,
                })
            })
            .transpose()?,
        merge: op.merge.as_ref().map(|m| pdpb::Merge {
            target: m.target.clone().map(region_to_proto),
        }),
        split_region: op.split_region.as_ref().map(|s| pdpb::SplitRegion {
            policy: s.policy.as_raw(),
            keys: s.keys.clone(),
        }),
        change_peer_v2: op
            .change_peer_v2
            .as_ref()
            .map(|v| {
                Ok(pdpb::ChangePeerV2 {
                    changes: repeated(&v.changes, "change_peer_v2.changes", change)?,
                })
            })
            .transpose()?,
        switch_witnesses: op
            .switch_witnesses
            .as_ref()
            .map(|v| {
                Ok(pdpb::BatchSwitchWitness {
                    switch_witnesses: repeated(
                        &v.switch_witnesses,
                        "switch_witnesses.switch_witnesses",
                        |w| pdpb::SwitchWitness {
                            peer_id: w.peer_id,
                            is_witness: w.is_witness,
                        },
                    )?,
                })
            })
            .transpose()?,
        change_split: op.change_split.as_ref().map(|s| pdpb::ChangeSplit {
            auto_split_enabled: s.auto_split_enabled,
        }),
    })
}
/// Convert to scheduling-service fields; its envelope uses different field tags.
pub fn to_scheduling(
    c: &HeartbeatCommand,
) -> Result<schedulingpb::RegionHeartbeatResponse, WireError> {
    let p = to_pdpb(c)?;
    Ok(schedulingpb::RegionHeartbeatResponse {
        header: Some(schedulingpb::ResponseHeader {
            cluster_id: c.cluster_id,
            error: None,
        }),
        region_id: p.region_id,
        region_epoch: p.region_epoch,
        target_peer: p.target_peer,
        change_peer: p.change_peer,
        transfer_leader: p.transfer_leader,
        merge: p.merge,
        split_region: p.split_region,
        change_peer_v2: p.change_peer_v2,
        switch_witnesses: p.switch_witnesses,
        change_split: p.change_split,
    })
}
/// Build Go's classic SendErr response; error enums are retained as raw values.
/// Go SendErr always emits PD, even on a scheduling-service stream.
pub fn error_pdpb(
    cluster_id: u64,
    error_type: i32,
    message: String,
    target: Option<Peer>,
) -> pdpb::RegionHeartbeatResponse {
    pdpb::RegionHeartbeatResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id,
            error: Some(pdpb::Error {
                r#type: error_type,
                message,
            }),
        }),
        target_peer: target.map(peer_to_proto),
        ..Default::default()
    }
}
/// Classic stream keepalive has only a present response header.
pub fn keepalive_pdpb(cluster_id: u64) -> pdpb::RegionHeartbeatResponse {
    pdpb::RegionHeartbeatResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id,
            error: None,
        }),
        ..Default::default()
    }
}
/// Scheduling stream keepalive has only a present response header.
pub fn keepalive_scheduling(cluster_id: u64) -> schedulingpb::RegionHeartbeatResponse {
    schedulingpb::RegionHeartbeatResponse {
        header: Some(schedulingpb::ResponseHeader {
            cluster_id,
            error: None,
        }),
        ..Default::default()
    }
}
