// Copyright 2026 AsterSQL.
//! Owned scheduling commands; protocol conversion belongs to the server adapter.
use astersql_pd_core::{Peer, RegionEpoch, RegionInfo, RegionMeta};

/// Membership change, preserving unrecognized adapter values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChangeType {
    #[default]
    AddNode,
    RemoveNode,
    AddLearnerNode,
    Unknown(i32),
}
impl ChangeType {
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::AddNode,
            1 => Self::RemoveNode,
            2 => Self::AddLearnerNode,
            n => Self::Unknown(n),
        }
    }
    pub fn as_raw(self) -> i32 {
        match self {
            Self::AddNode => 0,
            Self::RemoveNode => 1,
            Self::AddLearnerNode => 2,
            Self::Unknown(n) => n,
        }
    }
}

/// Split selection policy; explicit keys retain byte order and duplicates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitPolicy {
    #[default]
    Scan,
    Approximate,
    UseKey,
    Unknown(i32),
}
impl SplitPolicy {
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::Scan,
            1 => Self::Approximate,
            2 => Self::UseKey,
            n => Self::Unknown(n),
        }
    }
    pub fn as_raw(self) -> i32 {
        match self {
            Self::Scan => 0,
            Self::Approximate => 1,
            Self::UseKey => 2,
            Self::Unknown(n) => n,
        }
    }
}

/// One membership change. An absent peer is distinct from a zero peer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangePeer {
    pub peer: Option<Peer>,
    pub change_type: ChangeType,
}
/// Leader target and ordered candidate peers, including absent entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferLeader {
    pub peer: Option<Peer>,
    pub peers: Vec<Option<Peer>>,
}
/// Merge target, reusing the canonical owned region metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merge {
    pub target: Option<RegionMeta>,
}
/// Split policy and binary split keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitRegion {
    pub policy: SplitPolicy,
    pub keys: Vec<Vec<u8>>,
}
/// Ordered joint membership changes; an empty present batch leaves joint state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangePeerV2 {
    pub changes: Vec<Option<ChangePeer>>,
}
/// One witness conversion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchWitness {
    pub peer_id: u64,
    pub is_witness: bool,
}
/// Ordered witness conversions, preserving absent entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchSwitchWitness {
    pub switch_witnesses: Vec<Option<SwitchWitness>>,
}
/// Explicit automatic split setting; false still represents a present action.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSplit {
    pub auto_split_enabled: bool,
}
/// Independent optional actions, matching Go Operation rather than a one-of.
/// Empty and concurrently present actions are preserved without validation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Operation {
    pub change_peer: Option<ChangePeer>,
    pub transfer_leader: Option<TransferLeader>,
    pub merge: Option<Merge>,
    pub split_region: Option<SplitRegion>,
    pub change_peer_v2: Option<ChangePeerV2>,
    pub switch_witnesses: Option<BatchSwitchWitness>,
    pub change_split: Option<ChangeSplit>,
}
impl Operation {
    /// Whether every action is absent. Present empty actions are not empty.
    pub fn is_empty(&self) -> bool {
        self.change_peer.is_none()
            && self.transfer_leader.is_none()
            && self.merge.is_none()
            && self.split_region.is_none()
            && self.change_peer_v2.is_none()
            && self.switch_witnesses.is_none()
            && self.change_split.is_none()
    }
}

/// A command addressed to the observed leader and region epoch.
/// Owns its payload so queuing cannot observe later changes to input metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeartbeatCommand {
    pub cluster_id: u64,
    pub region_id: u64,
    pub region_epoch: Option<RegionEpoch>,
    pub target_peer: Peer,
    pub operation: Operation,
}
impl HeartbeatCommand {
    /// Snapshot a sendable command; a region without a leader suppresses sending.
    pub fn for_region(cluster_id: u64, region: &RegionInfo, operation: Operation) -> Option<Self> {
        let target_peer = region.leader()?.clone();
        Some(Self {
            cluster_id,
            region_id: region.meta().id,
            region_epoch: region.meta().epoch,
            target_peer,
            operation,
        })
    }
}
