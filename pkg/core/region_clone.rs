// Copyright 2026 AsterSQL.
//! Ordered clone modifications applied to owned domain snapshots.
use crate::{Peer, RegionInfo};

/// Metadata and membership modifications, in caller-supplied order.
#[derive(Clone, Debug)]
pub enum RegionOption {
    /// Set the decimal flow rounding precision.
    FlowRoundByDigit(i64),
    RegionId(u64),
    /// Reassign IDs in metadata order; a length mismatch is a no-op.
    PeerIds(Vec<u64>),
    Witnesses(Vec<u64>),
    /// Replace matching peers by learner identities, clearing witness status.
    Learners(Vec<Peer>),
    Role(u64, crate::PeerRole),
    DownPeers(Vec<crate::DownPeer>),
    PendingPeers(Vec<Peer>),
    ConfVer(u64),
    Version(u64),
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
                RegionOption::RegionId(id) => copy.meta.id = id,
                RegionOption::PeerIds(ids) => {
                    if ids.len() == copy.meta.peers.len() {
                        for (peer, id) in copy.meta.peers.iter_mut().zip(ids) {
                            peer.id = id;
                        }
                    }
                }
                RegionOption::Witnesses(ids) => {
                    for peer in &mut copy.meta.peers {
                        if ids.contains(&peer.id) {
                            peer.is_witness = true;
                        }
                    }
                }
                RegionOption::Learners(peers) => {
                    for peer in &mut copy.meta.peers {
                        if let Some(learner) = peers.iter().find(|p| p.id == peer.id) {
                            *peer = Peer {
                                id: learner.id,
                                store_id: learner.store_id,
                                role: crate::PeerRole::Learner,
                                is_witness: false,
                            };
                        }
                    }
                }
                RegionOption::Role(id, role) => {
                    for peer in &mut copy.meta.peers {
                        if peer.id == id {
                            peer.role = role;
                        }
                    }
                }
                RegionOption::DownPeers(peers) => copy.down_peers = peers,
                RegionOption::PendingPeers(peers) => copy.pending_peers = peers,
                RegionOption::ConfVer(value) => {
                    copy.meta
                        .epoch
                        .get_or_insert(crate::RegionEpoch {
                            conf_ver: 0,
                            version: 1,
                        })
                        .conf_ver = value;
                }
                RegionOption::Version(value) => {
                    copy.meta
                        .epoch
                        .get_or_insert(crate::RegionEpoch {
                            conf_ver: 1,
                            version: 0,
                        })
                        .version = value;
                }

                RegionOption::FlowRoundByDigit(digit) => {
                    copy.stats.flow_round_divisor = crate::flow_round_divisor_by_digit(digit)
                }
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
        let stats = copy.stats.clone();
        let mut result = Self::new(copy.meta, copy.leader, copy.down_peers, copy.pending_peers);
        result.term = copy.term;
        result.replication_status = copy.replication_status;
        result.replication_status_present = copy.replication_status_present;
        result.response_cpu_usage = copy.response_cpu_usage;
        result.report_buckets = copy.report_buckets;
        result.bucket_meta = copy.bucket_meta;
        result.stats = stats;
        result.query_stats = copy.query_stats;
        result.report_interval = copy.report_interval;
        result.read_cpu_usage = copy.read_cpu_usage;
        result
    }
}
