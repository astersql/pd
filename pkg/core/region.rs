// Copyright 2026 AsterSQL.
//! Owned region membership and peer queries without protocol dependencies.
use crate::{Peer, RegionMeta};
use std::collections::{BTreeMap, BTreeSet};

/// A down replica and the duration reported by its leader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownPeer {
    pub peer: Peer,
    pub down_seconds: u64,
}

/// Region metadata and classified membership. Queries cannot invalidate classification.
#[derive(Clone, Debug)]
pub struct RegionInfo {
    pub(crate) query_stats: Option<crate::QueryStats>,
    pub(crate) report_interval: (u64, u64),
    pub(crate) read_cpu_usage: u64,
    pub(crate) stats: crate::RegionStats,
    pub(crate) meta: RegionMeta,
    pub(crate) leader: Option<Peer>,
    voters: Vec<Peer>,
    learners: Vec<Peer>,
    witnesses: Vec<Peer>,
    pub(crate) down_peers: Vec<DownPeer>,
    pub(crate) pending_peers: Vec<Peer>,
}
impl RegionInfo {
    /// Construct membership, sorting classified and reported peers by peer ID.
    /// Every role except Learner belongs to the Go region voter classification.
    pub fn new(
        meta: RegionMeta,
        leader: Option<Peer>,
        mut down_peers: Vec<DownPeer>,
        mut pending_peers: Vec<Peer>,
    ) -> Self {
        let mut voters = Vec::new();
        let mut learners = Vec::new();
        let mut witnesses = Vec::new();
        for peer in &meta.peers {
            if peer.is_learner() {
                learners.push(peer.clone());
            } else {
                voters.push(peer.clone());
            }
            if peer.is_witness {
                witnesses.push(peer.clone());
            }
        }
        voters.sort_by_key(|p| p.id);
        learners.sort_by_key(|p| p.id);
        witnesses.sort_by_key(|p| p.id);
        down_peers.sort_by_key(|p| p.peer.id);
        pending_peers.sort_by_key(|p| p.id);
        Self {
            query_stats: None,
            report_interval: (0, 0),
            read_cpu_usage: 0,
            stats: crate::RegionStats::default(),
            meta,
            leader,
            voters,
            learners,
            witnesses,
            down_peers,
            pending_peers,
        }
    }
    /// Return the region meta.
    pub fn meta(&self) -> &RegionMeta {
        &self.meta
    }
    /// Return the region leader.
    pub fn leader(&self) -> Option<&Peer> {
        self.leader.as_ref()
    }
    /// Return the region voters.
    pub fn voters(&self) -> &[Peer] {
        &self.voters
    }
    /// Return the region learners.
    pub fn learners(&self) -> &[Peer] {
        &self.learners
    }
    /// Return the region witnesses.
    pub fn witnesses(&self) -> &[Peer] {
        &self.witnesses
    }
    /// Return the region down peers.
    pub fn down_peers(&self) -> &[DownPeer] {
        &self.down_peers
    }
    /// Return the region pending peers.
    pub fn pending_peers(&self) -> &[Peer] {
        &self.pending_peers
    }
    /// Find the first matching peer; return None when absent.
    pub fn get_peer(&self, id: u64) -> Option<&Peer> {
        self.meta.peers.iter().find(|p| p.id == id)
    }
    /// Find the first matching store_peer; return None when absent.
    pub fn get_store_peer(&self, id: u64) -> Option<&Peer> {
        self.meta.peers.iter().find(|p| p.store_id == id)
    }
    /// Find the first matching store_voter; return None when absent.
    pub fn get_store_voter(&self, id: u64) -> Option<&Peer> {
        self.voters.iter().find(|p| p.store_id == id)
    }
    /// Find the first matching store_learner; return None when absent.
    pub fn get_store_learner(&self, id: u64) -> Option<&Peer> {
        self.learners.iter().find(|p| p.store_id == id)
    }
    /// Find the first matching store_witness; return None when absent.
    pub fn get_store_witness(&self, id: u64) -> Option<&Peer> {
        self.witnesses.iter().find(|p| p.store_id == id)
    }
    /// Find the first matching down_peer; return None when absent.
    pub fn get_down_peer(&self, id: u64) -> Option<&Peer> {
        self.down_peers
            .iter()
            .map(|down| &down.peer)
            .find(|p| p.id == id)
    }
    /// Find the first matching down_voter; return None when absent.
    pub fn get_down_voter(&self, id: u64) -> Option<&Peer> {
        self.down_peers
            .iter()
            .map(|down| &down.peer)
            .find(|p| p.id == id && !p.is_learner())
    }
    /// Find the first matching down_learner; return None when absent.
    pub fn get_down_learner(&self, id: u64) -> Option<&Peer> {
        self.down_peers
            .iter()
            .map(|down| &down.peer)
            .find(|p| p.id == id && p.is_learner())
    }
    /// Find the first matching pending_peer; return None when absent.
    pub fn get_pending_peer(&self, id: u64) -> Option<&Peer> {
        self.pending_peers.iter().find(|p| p.id == id)
    }
    /// Find the first matching pending_voter; return None when absent.
    pub fn get_pending_voter(&self, id: u64) -> Option<&Peer> {
        self.pending_peers
            .iter()
            .find(|p| p.id == id && !p.is_learner())
    }
    /// Find the first matching pending_learner; return None when absent.
    pub fn get_pending_learner(&self, id: u64) -> Option<&Peer> {
        self.pending_peers
            .iter()
            .find(|p| p.id == id && p.is_learner())
    }
    /// Unique store IDs, including learners and witnesses.
    pub fn store_ids(&self) -> BTreeSet<u64> {
        self.meta.peers.iter().map(|p| p.store_id).collect()
    }
    /// Voters other than the leader, keyed by store; later sorted peers overwrite duplicates.
    pub fn followers(&self) -> BTreeMap<u64, &Peer> {
        self.voters
            .iter()
            .filter(|p| self.leader.as_ref().is_none_or(|l| l.id != p.id))
            .map(|p| (p.store_id, p))
            .collect()
    }
    /// Voters that hold full replicas, keyed by store.
    pub fn non_witness_voters(&self) -> BTreeMap<u64, &Peer> {
        self.voters
            .iter()
            .filter(|p| !p.is_witness)
            .map(|p| (p.store_id, p))
            .collect()
    }
    /// Followers on stores absent from the other region's followers.
    /// The result order is deterministic; Go map iteration makes no order guarantee.
    pub fn diff_followers(&self, other: &Self) -> Vec<&Peer> {
        let other = other.followers();
        self.followers()
            .into_iter()
            .filter(|(id, _)| !other.contains_key(id))
            .map(|(_, p)| p)
            .collect()
    }
}
