// Copyright 2026 AsterSQL.
//! Peer membership classification, independent of protocol representations.

/// A peer's membership role, including unrecognized roles from adapters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PeerRole {
    #[default]
    Voter,
    Learner,
    IncomingVoter,
    DemotingVoter,
    /// Preserve an unknown role without treating it as a voter or learner.
    Unknown(i32),
}

/// A replica's identity and membership; witness status is independent of role.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: u64,
    pub store_id: u64,
    pub role: PeerRole,
    pub is_witness: bool,
}

impl Peer {
    /// Whether this peer is a stable voter.
    pub fn is_voter(&self) -> bool {
        self.role == PeerRole::Voter
    }

    /// Whether this peer is a learner.
    pub fn is_learner(&self) -> bool {
        self.role == PeerRole::Learner
    }

    /// Whether this peer holds witness data rather than a full replica.
    pub fn is_witness(&self) -> bool {
        self.is_witness
    }

    /// Whether this peer is or will become a voter.
    pub fn is_voter_or_incoming_voter(&self) -> bool {
        matches!(self.role, PeerRole::Voter | PeerRole::IncomingVoter)
    }

    /// Whether this peer is or will become a learner.
    pub fn is_learner_or_demoting_voter(&self) -> bool {
        matches!(self.role, PeerRole::Learner | PeerRole::DemotingVoter)
    }
}

/// Whether any peer is entering or leaving the voter set.
pub fn is_in_joint_state(peers: &[Peer]) -> bool {
    peers
        .iter()
        .any(|peer| matches!(peer.role, PeerRole::IncomingVoter | PeerRole::DemotingVoter))
}

/// Count peers entering or leaving the voter set, including repeated entries.
pub fn count_in_joint_state(peers: &[Peer]) -> usize {
    peers
        .iter()
        .filter(|peer| matches!(peer.role, PeerRole::IncomingVoter | PeerRole::DemotingVoter))
        .count()
}
