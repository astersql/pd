// Copyright 2026 AsterSQL.
//! Ordered clone modifications applied to owned domain snapshots.
use crate::{Peer, RegionInfo};

/// Metadata and membership modifications, in caller-supplied order.
#[derive(Clone, Debug)]
pub enum RegionOption {
    StartKey(Vec<u8>),
    EndKey(Vec<u8>),
    Leader(Option<Peer>),
    Peers(Vec<Peer>),
    AddPeer(Peer),
    RemoveStorePeer(u64),
    ReplacePeerStore(u64, u64),
    ReplaceLeaderStore(u64),
    IncVersion,
    DecVersion,
    IncConfVer,
    DecConfVer,
    Flashback(bool, u64),
}
impl RegionInfo {
    /// Deep clone a snapshot, apply options in order, then rebuild classifications.
    /// Removing a peer does not implicitly clear the separately owned leader.
    pub fn clone_with(&self, options: impl IntoIterator<Item = RegionOption>) -> Self {
        let mut copy = self.clone();
        for option in options {
            match option {
                RegionOption::StartKey(key) => copy.meta.key_range.start_key = key,
                RegionOption::EndKey(key) => copy.meta.key_range.end_key = key,
                RegionOption::Leader(leader) => copy.leader = leader,
                RegionOption::Peers(peers) => copy.meta.peers = peers,
                RegionOption::AddPeer(peer) => copy.meta.peers.push(peer),
                RegionOption::RemoveStorePeer(store) => {
                    copy.meta.peers.retain(|p| p.store_id != store)
                }
                RegionOption::ReplacePeerStore(old, new) => {
                    for peer in &mut copy.meta.peers {
                        if peer.store_id == old {
                            peer.store_id = new;
                        }
                    }
                }
                RegionOption::ReplaceLeaderStore(store) => {
                    if let Some(peer) = copy
                        .meta
                        .peers
                        .iter()
                        .find(|p| p.store_id == store && !p.is_learner())
                    {
                        copy.leader = Some(peer.clone());
                    }
                }
                RegionOption::IncVersion => {
                    let epoch = copy.meta.epoch.get_or_insert_default();
                    epoch.version = epoch.version.wrapping_add(1);
                }
                RegionOption::IncConfVer => {
                    let epoch = copy.meta.epoch.get_or_insert_default();
                    epoch.conf_ver = epoch.conf_ver.wrapping_add(1);
                }
                RegionOption::DecVersion => {
                    if let Some(epoch) = &mut copy.meta.epoch {
                        epoch.version = epoch.version.wrapping_sub(1);
                    }
                }
                RegionOption::DecConfVer => {
                    if let Some(epoch) = &mut copy.meta.epoch {
                        epoch.conf_ver = epoch.conf_ver.wrapping_sub(1);
                    }
                }
                RegionOption::Flashback(active, ts) => {
                    copy.meta.is_in_flashback = active;
                    copy.meta.flashback_start_ts = ts;
                }
            }
        }
        Self::new(copy.meta, copy.leader, copy.down_peers, copy.pending_peers)
    }
}
