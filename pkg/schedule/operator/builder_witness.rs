// Copyright 2026 AsterSQL.
//! Witness intent and conversion diff shared by ordinary and joint planners.
use crate::{Builder, BuilderError};
use astersql_pd_core::{Peer, PeerRole};
use std::collections::BTreeMap;

/// Conversion work is derived from immutable origin/target snapshots. Existing
/// peer IDs always come from the origin; voter restoration follows snapshot catch-up.
#[derive(Default)]
pub(crate) struct WitnessPlan {
    pub witnesses: BTreeMap<u64, Peer>,
    pub non_witnesses: BTreeMap<u64, Peer>,
    pub restore: BTreeMap<u64, Peer>,
}
impl WitnessPlan {
    pub fn peer(&mut self, origin: &Peer, target: &Peer) -> Peer {
        let mut n = target.clone();
        n.id = origin.id;
        if origin.is_witness && !n.is_witness {
            if !n.is_learner() {
                self.restore.insert(n.store_id, n.clone());
                n.role = PeerRole::Learner;
                n.is_witness = true;
            }
            self.non_witnesses.insert(n.store_id, n.clone());
        } else if !origin.is_witness && n.is_witness {
            self.witnesses.insert(n.store_id, n.clone());
        }
        n
    }
    pub fn len(&self) -> usize {
        self.witnesses.len() + self.non_witnesses.len() + self.restore.len()
    }
}
impl<V> Builder<'_, V> {
    /// Record a witness conversion, preserving peer identity and role.
    pub fn become_witness(&mut self, store: u64) -> &mut Self {
        self.switch_witness(store, true)
    }
    /// Record a data-peer conversion; planning restores voter role after catch-up.
    pub fn become_non_witness(&mut self, store: u64) -> &mut Self {
        self.switch_witness(store, false)
    }
    fn switch_witness(&mut self, store: u64, witness: bool) -> &mut Self {
        if self.error().is_some() {
            return self;
        }
        match self.target_peers.get_mut(&store) {
            None => self.reject(BuilderError::MissingPeer(store)),
            Some(p) if p.is_witness == witness => self.reject(if witness {
                BuilderError::AlreadyWitness(store)
            } else {
                BuilderError::AlreadyNonWitness(store)
            }),
            Some(p) => p.is_witness = witness,
        }
        self
    }
}
