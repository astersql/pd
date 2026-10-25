// Copyright 2026 AsterSQL.
//! Atomic joint membership transitions over heartbeat domain state.
use crate::{DemoteVoter, OpInfluence, PromoteLearner, TransferLeader};
use astersql_pd_core::{
    BasicCluster, PeerRole, RegionInfo, count_in_joint_state, is_in_joint_state,
};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{ChangePeerV2, Operation};
use std::{fmt, time::Duration};

/// Enter joint consensus, promoting learners before demoting voters in the command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangePeerV2Enter {
    pub promote_learners: Vec<PromoteLearner>,
    pub demote_voters: Vec<DemoteVoter>,
}
/// Leave joint consensus after transferring any demoting leader away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangePeerV2Leave {
    pub promote_learners: Vec<PromoteLearner>,
    pub demote_voters: Vec<DemoteVoter>,
}
/// Stable role and atomicity failures, in the same priority as the Go checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointStepError {
    MissingPeer,
    PromoteDemotingVoter,
    DemoteIncomingVoter,
    UnexpectedRole,
    StillLearner,
    StillVoter,
    NonAtomic,
    OtherJointWhileJoint,
    OtherJointWhileNotJoint,
    DemoteLeader,
}
impl fmt::Display for JointStepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingPeer => "peer does not exist",
            Self::PromoteDemotingVoter => "cannot promote a demoting voter",
            Self::DemoteIncomingVoter => "cannot demote a incoming voter",
            Self::UnexpectedRole => "unexpected peer role",
            Self::StillLearner => "peer is still a learner",
            Self::StillVoter => "peer is still a voter",
            Self::NonAtomic => "non-atomic joint consensus",
            Self::OtherJointWhileJoint => {
                "some other peers are in joint state, when the region is in joint state"
            }
            Self::OtherJointWhileNotJoint => {
                "some other peers are in joint state, when the region is not in joint state"
            }
            Self::DemoteLeader => "cannot demote leader peer",
        })
    }
}
impl std::error::Error for JointStepError {}

fn check(
    promote: &[PromoteLearner],
    demote: &[DemoteVoter],
    r: &RegionInfo,
    leave: bool,
) -> Result<(), JointStepError> {
    use JointStepError::*;
    let (mut joint, mut stable, mut demote_leader) = (false, false, false);
    for pl in promote {
        // Match protobuf nil getters, including the zero-ID/default-role edge case.
        let p = r.get_store_peer(pl.to_store).cloned().unwrap_or_default();
        if p.id != pl.peer_id {
            return Err(MissingPeer);
        }
        match p.role {
            PeerRole::Voter => stable = true,
            PeerRole::Learner if !leave => stable = true,
            PeerRole::Learner => return Err(StillLearner),
            PeerRole::IncomingVoter => joint = true,
            PeerRole::DemotingVoter => return Err(PromoteDemotingVoter),
            _ => return Err(UnexpectedRole),
        }
    }
    for dv in demote {
        let p = r.get_store_peer(dv.to_store).cloned().unwrap_or_default();
        if p.id != dv.peer_id {
            return Err(MissingPeer);
        }
        match p.role {
            PeerRole::Learner => stable = true,
            PeerRole::Voter if !leave => stable = true,
            PeerRole::Voter => return Err(StillVoter),
            PeerRole::DemotingVoter => {
                joint = true;
                demote_leader |= p.store_id == r.leader().map_or(0, |p| p.store_id);
            }
            PeerRole::IncomingVoter => return Err(DemoteIncomingVoter),
            _ => return Err(UnexpectedRole),
        }
    }
    let count = count_in_joint_state(&r.meta().peers);
    if stable && joint {
        Err(NonAtomic)
    } else if stable && count != 0 {
        Err(OtherJointWhileJoint)
    } else if joint && count != promote.len() + demote.len() {
        Err(OtherJointWhileNotJoint)
    } else if leave && demote_leader {
        Err(DemoteLeader)
    } else {
        Ok(())
    }
}
fn describe(
    f: &mut fmt::Formatter<'_>,
    prefix: &str,
    p: &[PromoteLearner],
    d: &[DemoteVoter],
) -> fmt::Result {
    f.write_str(prefix)?;
    for pl in p {
        write!(
            f,
            ", promote learner peer {} on store {} to voter",
            pl.peer_id, pl.to_store
        )?;
    }
    for dv in d {
        write!(
            f,
            ", demote voter peer {} on store {} to learner",
            dv.peer_id, dv.to_store
        )?;
    }
    Ok(())
}
impl ChangePeerV2Enter {
    /// Confver acknowledgement tolerates removed demotions and witness-to-data changes.
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        if !self.promote_learners.iter().all(|pl| {
            let p = r.get_store_voter(pl.to_store).cloned().unwrap_or_default();
            p.id == pl.peer_id && p.is_voter_or_incoming_voter()
        }) {
            return 0;
        }
        if !self.demote_voters.iter().all(|dv| {
            r.get_store_peer(dv.to_store).is_none_or(|p| {
                p.id == dv.peer_id
                    && (p.is_learner_or_demoting_voter() || (dv.is_witness && !p.is_witness))
            })
        }) {
            return 0;
        }
        (self.promote_learners.len() + self.demote_voters.len()) as u64
    }
    /// Pending peers do not block membership acknowledgement; removed demotions do.
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        self.promote_learners.iter().all(|pl| {
            let p = r.get_store_voter(pl.to_store).cloned().unwrap_or_default();
            p.id == pl.peer_id && p.is_voter_or_incoming_voter()
        }) && self.demote_voters.iter().all(|dv| {
            r.get_store_peer(dv.to_store)
                .is_some_and(|p| p.id == dv.peer_id && p.is_learner_or_demoting_voter())
        })
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _cluster: &BasicCluster,
        _config: &C,
        r: &RegionInfo,
        _health: bool,
        _now: i128,
    ) -> Result<(), JointStepError> {
        check(&self.promote_learners, &self.demote_voters, r, false)
    }
    /// Only V2 supports joint consensus. Preserve member order and duplicates.
    pub fn get_cmd(&self, r: &RegionInfo, v2: bool) -> Option<Operation> {
        if !v2 {
            return None;
        }
        let mut changes =
            Vec::with_capacity(self.promote_learners.len() + self.demote_voters.len());
        for pl in &self.promote_learners {
            changes.extend(pl.get_cmd(r, true).change_peer_v2.unwrap().changes);
        }
        for dv in &self.demote_voters {
            changes.extend(dv.get_cmd(r, true).change_peer_v2.unwrap().changes);
        }
        Some(Operation {
            change_peer_v2: Some(ChangePeerV2 { changes }),
            ..Default::default()
        })
    }
}
impl ChangePeerV2Leave {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        if self
            .promote_learners
            .iter()
            .all(|p| p.conf_ver_changed(r) != 0)
            && self
                .demote_voters
                .iter()
                .all(|d| d.conf_ver_changed(r) != 0)
        {
            (self.promote_learners.len() + self.demote_voters.len()) as u64
        } else {
            0
        }
    }
    /// All target roles must be stable and the entire region must leave joint state.
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        self.promote_learners.iter().all(|p| p.is_finish(r))
            && self.demote_voters.iter().all(|d| d.is_finish(r))
            && !is_in_joint_state(&r.meta().peers)
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _cluster: &BasicCluster,
        _config: &C,
        r: &RegionInfo,
        _health: bool,
        _now: i128,
    ) -> Result<(), JointStepError> {
        check(&self.promote_learners, &self.demote_voters, r, true)
    }
    /// A present, empty V2 envelope instructs Raft to leave joint consensus.
    pub fn get_cmd(&self, _r: &RegionInfo, v2: bool) -> Option<Operation> {
        v2.then(|| Operation {
            change_peer_v2: Some(ChangePeerV2 { changes: vec![] }),
            ..Default::default()
        })
    }
}

impl ChangePeerV2Enter {
    /// Membership role changes have no store influence.
    pub fn influence(&self, _inf: &OpInfluence, _region: &RegionInfo) {}
    /// Signed Go time.Duration multiplication, including overflow.
    pub fn timeout_nanos(&self, size: i64) -> i64 {
        (TransferLeader::default().timeout(size).as_nanos() as i64).wrapping_mul(
            (self.promote_learners.len() as i64)
                .wrapping_add(self.demote_voters.len() as i64)
                .wrapping_add(1),
        )
    }
    /// Duration view retains the signed source timeout's two's-complement bits.
    pub fn timeout(&self, size: i64) -> Duration {
        Duration::from_nanos(self.timeout_nanos(size) as u64)
    }
}
impl fmt::Display for ChangePeerV2Enter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        describe(
            f,
            "use joint consensus",
            &self.promote_learners,
            &self.demote_voters,
        )
    }
}

impl ChangePeerV2Leave {
    /// Membership role changes have no store influence.
    pub fn influence(&self, _inf: &OpInfluence, _region: &RegionInfo) {}
    /// Signed Go time.Duration multiplication, including overflow.
    pub fn timeout_nanos(&self, size: i64) -> i64 {
        (TransferLeader::default().timeout(size).as_nanos() as i64).wrapping_mul(
            (self.promote_learners.len() as i64)
                .wrapping_add(self.demote_voters.len() as i64)
                .wrapping_add(1),
        )
    }
    /// Duration view retains the signed source timeout's two's-complement bits.
    pub fn timeout(&self, size: i64) -> Duration {
        Duration::from_nanos(self.timeout_nanos(size) as u64)
    }
}
impl fmt::Display for ChangePeerV2Leave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        describe(
            f,
            "leave joint state",
            &self.promote_learners,
            &self.demote_voters,
        )
    }
}
