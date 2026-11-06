// Copyright 2026 AsterSQL.
//! Shared wire assembly over canonical core snapshots, with no role or storage IO.
use crate::core_region_adapter::{peer_to_proto, region_to_proto};
use astersql_pd_proto::pdpb;
/// Convert a batched canonical query without changing misses or enum values.
pub fn query(
    header: pdpb::ResponseHeader,
    result: astersql_pd_core::RegionQueryResult,
) -> pdpb::QueryRegionResponse {
    let mut response = pdpb::QueryRegionResponse {
        header: Some(header),
        ..Default::default()
    };
    response.key_id_map = result.key_ids;
    response.prev_key_id_map = result.prev_key_ids;
    for (id, region) in result.regions_by_id {
        let wire = region
            .map(|r| pdpb::RegionResponse {
                region: Some(region_to_proto(r.region)),
                leader: r.leader.map(peer_to_proto),
                down_peers: r
                    .down_peers
                    .into_iter()
                    .map(|p| pdpb::PeerStats {
                        peer: Some(peer_to_proto(p.peer)),
                        down_seconds: p.down_seconds,
                    })
                    .collect(),
                pending_peers: r.pending_peers.into_iter().map(peer_to_proto).collect(),
                buckets: r.buckets.map(|b| astersql_pd_proto::metapb::Buckets {
                    region_id: b.region_id,
                    version: b.version,
                    keys: b.keys.clone(),
                    period_in_ms: b.period_in_ms,
                    stats: b
                        .stats
                        .as_ref()
                        .map(|s| astersql_pd_proto::metapb::BucketStats {
                            read_bytes: s.read_bytes.clone(),
                            read_keys: s.read_keys.clone(),
                            read_qps: s.read_qps.clone(),
                            write_bytes: s.write_bytes.clone(),
                            write_keys: s.write_keys.clone(),
                            write_qps: s.write_qps.clone(),
                        }),
                }),
            })
            .unwrap_or_default();
        response.regions_by_id.insert(id, wire);
    }
    response
}

/// Convert a scan item, retaining an explicit empty leader on a leader miss.
pub fn region(r: &astersql_pd_core::RegionInfo) -> pdpb::Region {
    pdpb::Region {
        region: Some(region_to_proto(r.meta().clone())),
        leader: Some(r.leader().cloned().map(peer_to_proto).unwrap_or_default()),
        down_peers: r
            .down_peers()
            .iter()
            .map(|p| pdpb::PeerStats {
                peer: Some(peer_to_proto(p.peer.clone())),
                down_seconds: p.down_seconds,
            })
            .collect(),
        pending_peers: r
            .pending_peers()
            .iter()
            .cloned()
            .map(peer_to_proto)
            .collect(),
        ..Default::default()
    }
}

/// Assemble a point response with a caller-owned header and bucket policy.
pub fn point(
    header: pdpb::ResponseHeader,
    region: Option<&astersql_pd_core::RegionInfo>,
    need_buckets: bool,
) -> pdpb::GetRegionResponse {
    let mut response = pdpb::GetRegionResponse {
        header: Some(header),
        ..Default::default()
    };
    if let Some(r) = region {
        response.region = Some(region_to_proto(r.meta().clone()));
        response.leader = r.leader().cloned().map(peer_to_proto);
        response.down_peers = r
            .down_peers()
            .iter()
            .map(|p| pdpb::PeerStats {
                peer: Some(peer_to_proto(p.peer.clone())),
                down_seconds: p.down_seconds,
            })
            .collect();
        response.pending_peers = r
            .pending_peers()
            .iter()
            .cloned()
            .map(peer_to_proto)
            .collect();
        if need_buckets {
            response.buckets = r.buckets().map(|b| astersql_pd_proto::metapb::Buckets {
                region_id: b.region_id,
                version: b.version,
                keys: b.keys,
                period_in_ms: b.period_in_ms,
                stats: b.stats.map(|s| astersql_pd_proto::metapb::BucketStats {
                    read_bytes: s.read_bytes,
                    read_keys: s.read_keys,
                    read_qps: s.read_qps,
                    write_bytes: s.write_bytes,
                    write_keys: s.write_keys,
                    write_qps: s.write_qps,
                }),
            });
        }
    }
    response
}
