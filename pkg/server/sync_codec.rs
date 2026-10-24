// Copyright 2026 AsterSQL.
//! Lossless owned conversions for the pinned SyncRegions protocol. Projection to
//! follower application deliberately follows Go's subset and length semantics.
use crate::core_region_adapter::{
    peer_from_proto, peer_to_proto, region_from_proto, region_to_proto,
};
use astersql_pd_core::{Peer, RegionBucketMeta, RegionBuckets, RegionInfo, RegionMeta};
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_statistics_buckets::BucketFlows;
use astersql_pd_syncer::{
    CatchUpFrame, HistoryIndex, SnapshotBatch, SyncApplyResponse, SyncFrame, SyncRegionStats,
    SyncResponseError,
};
use std::sync::Arc;

/// Owned protocol metadata; optional message presence is retained by the envelope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncRequestHeader {
    pub cluster_id: u64,
    pub sender_id: u64,
    pub caller_id: String,
    pub caller_component: String,
}
impl From<SyncRequestHeader> for pdpb::RequestHeader {
    fn from(p: SyncRequestHeader) -> Self {
        Self {
            cluster_id: p.cluster_id,
            sender_id: p.sender_id,
            caller_id: p.caller_id,
            caller_component: p.caller_component,
        }
    }
}
impl From<pdpb::RequestHeader> for SyncRequestHeader {
    fn from(p: pdpb::RequestHeader) -> Self {
        Self {
            cluster_id: p.cluster_id,
            sender_id: p.sender_id,
            caller_id: p.caller_id,
            caller_component: p.caller_component,
        }
    }
}

/// Owned protocol metadata; optional message presence is retained by the envelope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncMember {
    pub name: String,
    pub member_id: u64,
    pub peer_urls: Vec<String>,
    pub client_urls: Vec<String>,
    pub leader_priority: i32,
    pub deploy_path: String,
    pub binary_version: String,
    pub git_hash: String,
    pub dc_location: String,
}
impl From<SyncMember> for pdpb::Member {
    fn from(p: SyncMember) -> Self {
        Self {
            name: p.name,
            member_id: p.member_id,
            peer_urls: p.peer_urls,
            client_urls: p.client_urls,
            leader_priority: p.leader_priority,
            deploy_path: p.deploy_path,
            binary_version: p.binary_version,
            git_hash: p.git_hash,
            dc_location: p.dc_location,
        }
    }
}
impl From<pdpb::Member> for SyncMember {
    fn from(p: pdpb::Member) -> Self {
        Self {
            name: p.name,
            member_id: p.member_id,
            peer_urls: p.peer_urls,
            client_urls: p.client_urls,
            leader_priority: p.leader_priority,
            deploy_path: p.deploy_path,
            binary_version: p.binary_version,
            git_hash: p.git_hash,
            dc_location: p.dc_location,
        }
    }
}

/// Initial follower request. Index remains independent of MetaStore revision.
#[derive(Clone, Debug, Default)]
pub struct SyncRequest {
    pub header: Option<SyncRequestHeader>,
    pub member: Option<SyncMember>,
    pub start_index: HistoryIndex,
}
/// No admission, member lookup, or transport side effects occur in conversion.
pub fn request_from_proto(p: pdpb::SyncRegionRequest) -> SyncRequest {
    SyncRequest {
        header: p.header.map(Into::into),
        member: p.member.map(Into::into),
        start_index: HistoryIndex(p.start_index),
    }
}
/// Restore optional headers and every pinned member field.
pub fn request_to_proto(p: SyncRequest) -> pdpb::SyncRegionRequest {
    pdpb::SyncRegionRequest {
        header: p.header.map(Into::into),
        member: p.member.map(Into::into),
        start_index: p.start_index.0,
    }
}
/// Header presence and arbitrary error enum values survive roundtrip.
#[derive(Clone, Debug, Default)]
pub struct SyncHeader {
    pub cluster_id: u64,
    pub error: Option<SyncResponseError>,
}
/// Buckets retain all pinned fields, even noncanonical incoming stats/period.
#[derive(Clone, Debug, Default)]
pub struct SyncBuckets {
    pub meta: RegionBuckets,
    pub stats: Option<BucketFlows>,
    pub period_in_ms: u64,
}
/// Optional down-peer message and elapsed seconds, without a fabricated peer.
#[derive(Clone, Debug, Default)]
pub struct SyncDownPeer {
    pub peer: Option<Peer>,
    pub down_seconds: u64,
}
/// Protocol envelope with domain-owned values. Start is intentionally a raw u64:
/// full-scan positions must not become reusable history acknowledgements.
#[derive(Clone, Debug, Default)]
pub struct SyncResponse {
    pub header: Option<SyncHeader>,
    pub start_index: u64,
    pub regions: Vec<RegionMeta>,
    pub stats: Vec<SyncRegionStats>,
    pub buckets: Vec<SyncBuckets>,
    pub leaders: Vec<Peer>,
    pub down_peers: Vec<Vec<SyncDownPeer>>,
    pub pending_peers: Vec<Vec<Peer>>,
}
impl SyncResponse {
    /// Project exactly the Go follower-consumed fields. Keep original array
    /// lengths so ClientApply performs whole-array checks, not zip/truncation.
    /// ClientApply alone classifies full-sync positions and checks leader ID zero.
    pub fn into_apply(self) -> SyncApplyResponse {
        SyncApplyResponse {
            error: self.header.and_then(|h| h.error),
            start_index: HistoryIndex(self.start_index),
            regions: self.regions,
            stats: self.stats,
            buckets: self
                .buckets
                .into_iter()
                .map(|b| {
                    Some(RegionBucketMeta {
                        version: b.meta.version,
                        keys: b.meta.keys,
                    })
                })
                .collect(),
            leaders: self.leaders.into_iter().map(Some).collect(),
        }
    }
}
/// Decode without discarding mismatched arrays, zero leaders, or error frames.
pub fn response_from_proto(p: pdpb::SyncRegionResponse) -> SyncResponse {
    SyncResponse {
        header: p.header.map(|h| SyncHeader {
            cluster_id: h.cluster_id,
            error: h.error.map(|e| SyncResponseError {
                code: e.r#type,
                message: e.message,
            }),
        }),
        start_index: p.start_index,
        regions: p
            .regions
            .into_iter()
            .map(|r| region_from_proto(Some(r)).expect("present region"))
            .collect(),
        stats: p
            .region_stats
            .into_iter()
            .map(|s| SyncRegionStats {
                written_bytes: s.bytes_written,
                written_keys: s.keys_written,
                read_bytes: s.bytes_read,
                read_keys: s.keys_read,
            })
            .collect(),
        leaders: p.region_leaders.into_iter().map(peer_from_proto).collect(),
        buckets: p
            .buckets
            .into_iter()
            .map(|b| SyncBuckets {
                meta: RegionBuckets {
                    region_id: b.region_id,
                    version: b.version,
                    keys: b.keys,
                    ..Default::default()
                },
                period_in_ms: b.period_in_ms,
                stats: b.stats.map(|s| BucketFlows {
                    read_bytes: s.read_bytes,
                    read_keys: s.read_keys,
                    read_qps: s.read_qps,
                    write_bytes: s.write_bytes,
                    write_keys: s.write_keys,
                    write_qps: s.write_qps,
                }),
            })
            .collect(),
        down_peers: p
            .down_peers
            .into_iter()
            .map(|p| {
                p.peers
                    .into_iter()
                    .map(|p| SyncDownPeer {
                        peer: p.peer.map(peer_from_proto),
                        down_seconds: p.down_seconds,
                    })
                    .collect()
            })
            .collect(),
        pending_peers: p
            .pending_peers
            .into_iter()
            .map(|p| p.peers.into_iter().map(peer_from_proto).collect())
            .collect(),
    }
}
/// Encode all pinned fields and message presence; no runtime or registry needed.
pub fn response_to_proto(p: SyncResponse) -> pdpb::SyncRegionResponse {
    pdpb::SyncRegionResponse {
        header: p.header.map(|h| pdpb::ResponseHeader {
            cluster_id: h.cluster_id,
            error: h.error.map(|e| pdpb::Error {
                r#type: e.code,
                message: e.message,
            }),
        }),
        start_index: p.start_index,
        regions: p.regions.into_iter().map(region_to_proto).collect(),
        region_stats: p
            .stats
            .into_iter()
            .map(|s| pdpb::RegionStat {
                bytes_written: s.written_bytes,
                keys_written: s.written_keys,
                bytes_read: s.read_bytes,
                keys_read: s.read_keys,
            })
            .collect(),
        region_leaders: p.leaders.into_iter().map(peer_to_proto).collect(),
        buckets: p
            .buckets
            .into_iter()
            .map(|b| metapb::Buckets {
                region_id: b.meta.region_id,
                version: b.meta.version,
                keys: b.meta.keys,
                period_in_ms: b.period_in_ms,
                stats: b.stats.map(|s| metapb::BucketStats {
                    read_bytes: s.read_bytes,
                    read_keys: s.read_keys,
                    read_qps: s.read_qps,
                    write_bytes: s.write_bytes,
                    write_keys: s.write_keys,
                    write_qps: s.write_qps,
                }),
            })
            .collect(),
        down_peers: p
            .down_peers
            .into_iter()
            .map(|p| pdpb::PeersStats {
                peers: p
                    .into_iter()
                    .map(|p| pdpb::PeerStats {
                        peer: p.peer.map(peer_to_proto),
                        down_seconds: p.down_seconds,
                    })
                    .collect(),
            })
            .collect(),
        pending_peers: p
            .pending_peers
            .into_iter()
            .map(|p| pdpb::Peers {
                peers: p.into_iter().map(peer_to_proto).collect(),
            })
            .collect(),
    }
}
fn records_to_proto(
    cluster_id: u64,
    start_index: u64,
    records: &[Arc<RegionInfo>],
) -> pdpb::SyncRegionResponse {
    let mut p = pdpb::SyncRegionResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id,
            error: None,
        }),
        start_index,
        ..Default::default()
    };
    for r in records {
        p.regions.push(region_to_proto(r.meta().clone()));
        let s = r.stats();
        p.region_stats.push(pdpb::RegionStat {
            bytes_written: s.written_bytes,
            keys_written: s.written_keys,
            bytes_read: s.read_bytes,
            keys_read: s.read_keys,
        });
        p.region_leaders
            .push(r.leader().cloned().map(peer_to_proto).unwrap_or_default());
        p.buckets.push(
            r.buckets()
                .map(|b| metapb::Buckets {
                    region_id: b.region_id,
                    version: b.version,
                    keys: b.keys,
                    period_in_ms: b.period_in_ms,
                    stats: b.stats.map(|s| metapb::BucketStats {
                        read_bytes: s.read_bytes,
                        read_keys: s.read_keys,
                        read_qps: s.read_qps,
                        write_bytes: s.write_bytes,
                        write_keys: s.write_keys,
                        write_qps: s.write_qps,
                    }),
                })
                .unwrap_or_default(),
        );
    }
    p
}
/// Encode an incremental history frame, including an empty keepalive.
pub fn history_to_proto(cluster_id: u64, frame: &SyncFrame) -> pdpb::SyncRegionResponse {
    records_to_proto(cluster_id, frame.start_index().0, frame.records())
}
/// Encode a positional full-scan batch without changing any history index.
pub fn snapshot_to_proto(cluster_id: u64, batch: SnapshotBatch<'_>) -> pdpb::SyncRegionResponse {
    records_to_proto(cluster_id, batch.offset.0 as u64, batch.records)
}
/// Encode catch-up records or the captured history completion boundary.
pub fn catch_up_to_proto(cluster_id: u64, frame: CatchUpFrame<'_>) -> pdpb::SyncRegionResponse {
    match frame {
        CatchUpFrame::Records {
            start_index,
            records,
        } => records_to_proto(cluster_id, start_index, records),
        CatchUpFrame::Complete { next_index } => records_to_proto(cluster_id, next_index.0, &[]),
    }
}
