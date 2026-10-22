// Copyright 2026 AsterSQL.
//! HTTP peer JSON, using owned domain values rather than generated protocols.
use astersql_pd_core::{DownPeer, Peer, PeerRole};
use serde::Serialize;

fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Peer fields flattened into the HTTP API's compatible representation.
/// A missing peer has an empty role name, unlike an explicit default voter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MetaPeer {
    pub role_name: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub is_learner: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub id: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub store_id: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub role: i32,
    #[serde(skip_serializing_if = "is_zero")]
    pub is_witness: bool,
}

impl MetaPeer {
    /// Convert a nullable domain peer; missing peers use the repaired Go default.
    pub fn from_peer(peer: Option<&Peer>) -> Self {
        let Some(peer) = peer else {
            return Self::default();
        };
        let (role, role_name) = match peer.role {
            PeerRole::Voter => (0, "Voter".to_owned()),
            PeerRole::Learner => (1, "Learner".to_owned()),
            PeerRole::IncomingVoter => (2, "IncomingVoter".to_owned()),
            PeerRole::DemotingVoter => (3, "DemotingVoter".to_owned()),
            PeerRole::Unknown(n) => (
                n,
                match n {
                    0 => "Voter".to_owned(),
                    1 => "Learner".to_owned(),
                    2 => "IncomingVoter".to_owned(),
                    3 => "DemotingVoter".to_owned(),
                    _ => n.to_string(),
                },
            ),
        };
        Self {
            role_name,
            is_learner: role == 1,
            id: peer.id,
            store_id: peer.store_id,
            role,
            is_witness: peer.is_witness,
        }
    }
}

/// Down-peer HTTP representation. The peer object is always emitted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PDPeerStats {
    pub peer: MetaPeer,
    #[serde(skip_serializing_if = "is_zero")]
    pub down_seconds: u64,
}

impl From<&DownPeer> for PDPeerStats {
    fn from(stats: &DownPeer) -> Self {
        Self {
            peer: MetaPeer::from_peer(Some(&stats.peer)),
            down_seconds: stats.down_seconds,
        }
    }
}
