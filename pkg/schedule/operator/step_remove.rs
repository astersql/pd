// Copyright 2026 AsterSQL.
//! Peer removal and the learner acknowledgement used by joint-consensus demotion.
use std::{fmt, time::Duration};

use astersql_pd_core::{BasicCluster, Peer, PeerRole, RegionInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{ChangePeer, ChangePeerV2, ChangeType, Operation};
use astersql_pd_storelimit::{SMALL_REGION_THRESHOLD, Type};

use crate::{OpInfluence, TransferLeader};

/// Remove any peer at a store; the optional ID tracks configuration acknowledgement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemovePeer {
    pub from_store: u64,
    pub peer_id: u64,
    pub is_light_weight: bool,
    pub is_down_store: bool,
}

/// A joint-consensus substep that may demote the leader as well as a follower.
/// It is not a standalone OpStep and has no progress check or influence method.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DemoteVoter {
    pub to_store: u64,
    pub peer_id: u64,
    pub is_witness: bool,
}

/// Removal is rejected while the source store is the reported leader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovePeerError {
    LeaderPeer,
}
impl fmt::Display for RemovePeerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cannot remove leader peer")
    }
}
impl std::error::Error for RemovePeerError {}

fn command(change: ChangePeer, v2: bool) -> Operation {
    if v2 {
        Operation {
            change_peer_v2: Some(ChangePeerV2 {
                changes: vec![Some(change)],
            }),
            ..Default::default()
        }
    } else {
        Operation {
            change_peer: Some(change),
            ..Default::default()
        }
    }
}
impl RemovePeer {
    /// A replacement ID acknowledges removal even though that store remains peered.
    pub fn conf_ver_changed(&self, region: &RegionInfo) -> u64 {
        let id = region.get_store_peer(self.from_store).map_or(0, |p| p.id);
        u64::from(id == 0 || (self.peer_id != 0 && id != self.peer_id))
    }
    /// Completion requires the whole store peer to be absent, regardless of ID.
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        region.get_store_peer(self.from_store).is_none()
    }
    /// Store health and identity do not prevent removal; leader safety does.
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _cluster: &BasicCluster,
        _config: &C,
        region: &RegionInfo,
        _need_store_health_check: bool,
        _now_unix_nanos: i128,
    ) -> Result<(), RemovePeerError> {
        if self.from_store == region.leader().map_or(0, |p| p.store_id) {
            Err(RemovePeerError::LeaderPeer)
        } else {
            Ok(())
        }
    }
    /// Remove peer-local data/counts and charge nonwitness, nonlightweight work.
    pub fn influence(&self, inf: &OpInfluence, region: &RegionInfo) {
        let size = region.store_peer_approximate_size(self.from_store);
        inf.update_store(self.from_store, |s| {
            s.region_size = s.region_size.wrapping_sub(size);
            s.region_count = s.region_count.wrapping_sub(1);
            if region
                .get_store_peer(self.from_store)
                .is_some_and(|p| p.is_witness)
            {
                s.witness_count = s.witness_count.wrapping_sub(1);
                return;
            }
            if self.is_light_weight {
                return;
            }
            let cost_size = if self.is_down_store {
                size.min(SMALL_REGION_THRESHOLD)
            } else {
                size
            };
            s.adjust_step_cost(
                Type::REMOVE_PEER,
                cost_size,
                region.store_peer_approximate_keys(self.from_store),
            );
        });
    }
    pub fn timeout(&self, region_size: i64) -> Duration {
        TransferLeader::default().timeout(region_size)
    }
    /// Emit the heartbeat's current peer, including None after removal or a new ID.
    pub fn get_cmd(&self, region: &RegionInfo, v2: bool) -> Operation {
        command(
            ChangePeer {
                change_type: ChangeType::RemoveNode,
                peer: region.get_store_peer(self.from_store).cloned(),
            },
            v2,
        )
    }
}
impl DemoteVoter {
    /// A removed peer or witness-to-data conversion also acknowledges demotion.
    pub fn conf_ver_changed(&self, region: &RegionInfo) -> u64 {
        u64::from(region.get_store_peer(self.to_store).is_none_or(|p| {
            (p.id == self.peer_id && p.role == PeerRole::Learner)
                || (self.is_witness && !p.is_witness)
        }))
    }
    /// Only a matching learner completes this substep; pending learners are allowed.
    pub fn is_finish(&self, region: &RegionInfo) -> bool {
        region
            .get_store_learner(self.to_store)
            .is_some_and(|p| p.id == self.peer_id)
    }
    pub fn timeout(&self, region_size: i64) -> Duration {
        TransferLeader::default().timeout(region_size)
    }
    pub fn get_cmd(&self, _region: &RegionInfo, v2: bool) -> Operation {
        command(
            ChangePeer {
                change_type: ChangeType::AddLearnerNode,
                peer: Some(Peer {
                    id: self.peer_id,
                    store_id: self.to_store,
                    role: PeerRole::Learner,
                    is_witness: self.is_witness,
                }),
            },
            v2,
        )
    }
}
impl fmt::Display for RemovePeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "remove peer on store {}", self.from_store)
    }
}
impl fmt::Display for DemoteVoter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "demote {}voter peer {} on store {} to learner",
            if self.is_witness { "witness " } else { "" },
            self.peer_id,
            self.to_store
        )
    }
}
