// Copyright 2026 AsterSQL.
//! Heartbeat-driven peer addition and learner promotion.
use crate::{LeaderTargetError, OpInfluence, TransferLeader};
use astersql_pd_core::{BasicCluster, Peer, PeerRole, RegionInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{ChangePeer, ChangePeerV2, ChangeType, Operation};
use astersql_pd_storelimit::Type;
use std::{fmt, time::Duration};

/// Add a voter; an already present peer suppresses further commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddPeer {
    pub to_store: u64,
    pub peer_id: u64,
    pub is_light_weight: bool,
    pub is_witness: bool,
}
/// Add a learner, optionally charging a source store for the snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddLearner {
    pub to_store: u64,
    pub peer_id: u64,
    pub send_store: u64,
    pub is_light_weight: bool,
    pub is_witness: bool,
}
/// Promote a learner; completion requires the ordinary Voter role.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromoteLearner {
    pub to_store: u64,
    pub peer_id: u64,
    pub is_witness: bool,
}
/// Store health and peer identity failures, in source validation order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerStepError {
    Store(LeaderTargetError),
    ExistingPeer {
        existing: u64,
        store: u64,
        expected: u64,
    },
    AlreadyVoter,
    MissingPeer,
}
impl fmt::Display for PeerStepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => write!(f, "{e}"),
            Self::ExistingPeer {
                existing,
                store,
                expected,
            } => write!(
                f,
                "peer {existing} has already existed in store {store}, the timeout is trying to add peer {expected} on the same store"
            ),
            Self::AlreadyVoter => f.write_str("peer already is a voter"),
            Self::MissingPeer => f.write_str("peer does not exist"),
        }
    }
}
impl std::error::Error for PeerStepError {}
pub(super) fn validate<C: SharedConfigProvider>(
    cluster: &BasicCluster,
    config: &C,
    to: u64,
    health: bool,
    now: i128,
) -> Result<(), PeerStepError> {
    let Some(s) = cluster.stores.get_store(to) else {
        return Err(PeerStepError::Store(LeaderTargetError::MissingStore));
    };
    if health && s.is_unhealthy(now) {
        return Err(PeerStepError::Store(LeaderTargetError::Unhealthy));
    }
    if i128::from(s.down_time_nanos(now)) > config.get_max_store_down_time().as_nanos() as i128 {
        return Err(PeerStepError::Store(LeaderTargetError::Down));
    }
    Ok(())
}
fn check_peer(region: &RegionInfo, to: u64, id: u64, learner: bool) -> Result<(), PeerStepError> {
    if let Some(p) = region.get_store_peer(to) {
        if p.id != id {
            return Err(PeerStepError::ExistingPeer {
                existing: p.id,
                store: to,
                expected: id,
            });
        }
        if learner && !p.is_learner() {
            return Err(PeerStepError::AlreadyVoter);
        }
    }
    Ok(())
}
fn command(to: u64, id: u64, witness: bool, learner: bool, v2: bool) -> Operation {
    let c = ChangePeer {
        peer: Some(Peer {
            id,
            store_id: to,
            role: if learner {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: witness,
        }),
        change_type: if learner {
            ChangeType::AddLearnerNode
        } else {
            ChangeType::AddNode
        },
    };
    if v2 {
        Operation {
            change_peer_v2: Some(ChangePeerV2 {
                changes: vec![Some(c)],
            }),
            ..Default::default()
        }
    } else {
        Operation {
            change_peer: Some(c),
            ..Default::default()
        }
    }
}
fn add_influence(
    inf: &OpInfluence,
    r: &RegionInfo,
    to: u64,
    light: bool,
    witness: bool,
    send: u64,
) {
    let size = r.stats().approximate_size;
    inf.update_store(to, |s| {
        if witness {
            s.witness_count = s.witness_count.wrapping_add(1)
        } else {
            s.region_size = s.region_size.wrapping_add(size)
        }
        s.region_count = s.region_count.wrapping_add(1);
        if !light && !witness {
            s.adjust_step_cost(Type::ADD_PEER, size, r.stats().approximate_keys)
        }
    });
    if !light && !witness && send != 0 {
        inf.update_store(send, |s| s.add_step_cost(Type::SEND_SNAPSHOT, size))
    }
}
pub(super) fn slow_timeout(size: i64) -> Duration {
    Duration::from_nanos(
        size.wrapping_mul(6)
            .wrapping_mul(1_000_000_000)
            .max(600_000_000_000) as u64,
    )
}
impl AddPeer {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        u64::from(r.get_store_voter(self.to_store).map_or(0, |p| p.id) == self.peer_id)
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        r.get_store_voter(self.to_store).is_some_and(|p| {
            p.id == self.peer_id
                && p.is_witness == self.is_witness
                && r.get_pending_voter(p.id).is_none()
        })
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        health: bool,
        now: i128,
    ) -> Result<(), PeerStepError> {
        validate(c, cfg, self.to_store, health, now)?;
        check_peer(r, self.to_store, self.peer_id, false)
    }
    pub fn influence(&self, inf: &OpInfluence, r: &RegionInfo) {
        add_influence(
            inf,
            r,
            self.to_store,
            self.is_light_weight,
            self.is_witness,
            0,
        )
    }
    pub fn timeout(&self, size: i64) -> Duration {
        slow_timeout(size)
    }
    pub fn get_cmd(&self, r: &RegionInfo, v2: bool) -> Option<Operation> {
        r.get_store_peer(self.to_store)
            .is_none()
            .then(|| command(self.to_store, self.peer_id, self.is_witness, false, v2))
    }
}
impl AddLearner {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        u64::from(r.get_store_peer(self.to_store).map_or(0, |p| p.id) == self.peer_id)
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        r.get_store_learner(self.to_store).is_some_and(|p| {
            p.id == self.peer_id
                && p.is_witness == self.is_witness
                && r.get_pending_learner(p.id).is_none()
        })
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        health: bool,
        now: i128,
    ) -> Result<(), PeerStepError> {
        validate(c, cfg, self.to_store, health, now)?;
        check_peer(r, self.to_store, self.peer_id, true)
    }
    pub fn influence(&self, inf: &OpInfluence, r: &RegionInfo) {
        add_influence(
            inf,
            r,
            self.to_store,
            self.is_light_weight,
            self.is_witness,
            self.send_store,
        )
    }
    pub fn timeout(&self, size: i64) -> Duration {
        slow_timeout(size)
    }
    pub fn get_cmd(&self, r: &RegionInfo, v2: bool) -> Option<Operation> {
        r.get_store_peer(self.to_store)
            .is_none()
            .then(|| command(self.to_store, self.peer_id, self.is_witness, true, v2))
    }
}
impl PromoteLearner {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        u64::from(
            r.get_store_voter(self.to_store).map_or(0, |p| p.id) == self.peer_id
                && r.get_store_voter(self.to_store)
                    .map_or(PeerRole::Voter, |p| p.role)
                    == PeerRole::Voter,
        )
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        r.get_store_voter(self.to_store)
            .is_some_and(|p| p.id == self.peer_id && p.role == PeerRole::Voter)
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        _c: &BasicCluster,
        _cfg: &C,
        r: &RegionInfo,
        _health: bool,
        _now: i128,
    ) -> Result<(), PeerStepError> {
        if r.get_store_peer(self.to_store).map_or(0, |p| p.id) == self.peer_id {
            Ok(())
        } else {
            Err(PeerStepError::MissingPeer)
        }
    }
    pub fn influence(&self, _inf: &OpInfluence, _r: &RegionInfo) {}
    pub fn timeout(&self, size: i64) -> Duration {
        TransferLeader::default().timeout(size)
    }
    pub fn get_cmd(&self, _r: &RegionInfo, v2: bool) -> Operation {
        command(self.to_store, self.peer_id, self.is_witness, false, v2)
    }
}
impl fmt::Display for AddPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "add {}peer {} on store {}",
            if self.is_witness { "witness " } else { "" },
            self.peer_id,
            self.to_store
        )
    }
}
impl fmt::Display for AddLearner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "add {}learner peer {} on store {}",
            if self.is_witness { "witness " } else { "" },
            self.peer_id,
            self.to_store
        )
    }
}
impl fmt::Display for PromoteLearner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "promote {}learner peer {} on store {} to voter",
            if self.is_witness { "witness " } else { "" },
            self.peer_id,
            self.to_store
        )
    }
}
