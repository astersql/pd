// Copyright 2026 AsterSQL.
//! Owned protocol conversions. Heartbeat sizes enter core in bytes; normalization
//! occurs exactly once. Missing region/down-peer messages are rejected explicitly.
use astersql_pd_core as core;
use astersql_pd_proto::{metapb, pdpb};

/// Preserve membership role, including unknown wire enum values.
pub fn peer_from_proto(p: metapb::Peer) -> core::Peer {
    core::Peer {
        id: p.id,
        store_id: p.store_id,
        is_witness: p.is_witness,
        role: match p.role {
            0 => core::PeerRole::Voter,
            1 => core::PeerRole::Learner,
            2 => core::PeerRole::IncomingVoter,
            3 => core::PeerRole::DemotingVoter,
            n => core::PeerRole::Unknown(n),
        },
    }
}
/// Restore a peer without normalizing its role or witness flag.
pub fn peer_to_proto(p: core::Peer) -> metapb::Peer {
    metapb::Peer {
        id: p.id,
        store_id: p.store_id,
        is_witness: p.is_witness,
        role: match p.role {
            core::PeerRole::Voter => 0,
            core::PeerRole::Learner => 1,
            core::PeerRole::IncomingVoter => 2,
            core::PeerRole::DemotingVoter => 3,
            core::PeerRole::Unknown(n) => n,
        },
    }
}
/// Convert metadata, preserving optional epoch and encryption metadata presence.
pub fn region_from_proto(value: Option<metapb::Region>) -> Option<core::RegionMeta> {
    value.map(|p| core::RegionMeta {
        id: p.id,
        key_range: core::KeyRange {
            start_key: p.start_key,
            end_key: p.end_key,
        },
        epoch: p.region_epoch.map(|e| core::RegionEpoch {
            conf_ver: e.conf_ver,
            version: e.version,
        }),
        peers: p.peers.into_iter().map(peer_from_proto).collect(),
        encryption_meta: p.encryption_meta.map(|e| core::RegionEncryptionMeta {
            key_id: e.key_id,
            iv: e.iv,
        }),
        is_in_flashback: p.is_in_flashback,
        flashback_start_ts: p.flashback_start_ts,
    })
}
/// Restore all known metadata fields without interpreting encrypted range keys.
pub fn region_to_proto(p: core::RegionMeta) -> metapb::Region {
    metapb::Region {
        id: p.id,
        start_key: p.key_range.start_key,
        end_key: p.key_range.end_key,
        region_epoch: p.epoch.map(|e| metapb::RegionEpoch {
            conf_ver: e.conf_ver,
            version: e.version,
        }),
        peers: p.peers.into_iter().map(peer_to_proto).collect(),
        encryption_meta: p.encryption_meta.map(|e| {
            astersql_pd_proto::encryptionpb::EncryptionMeta {
                key_id: e.key_id,
                iv: e.iv,
            }
        }),
        is_in_flashback: p.is_in_flashback,
        flashback_start_ts: p.flashback_start_ts,
    }
}
/// Assemble a heartbeat snapshot and apply core's Go-compatible normalization.
/// Request headers belong to the transport host. Deprecated CPU and scheduler CPU
/// fields are not scheduling loads; unified-read CPU is the domain load counter.
pub fn region_from_heartbeat(
    p: pdpb::RegionHeartbeatRequest,
    divisor: u64,
) -> Result<core::RegionInfo, &'static str> {
    let meta = region_from_proto(p.region).ok_or("missing region")?;
    let down = p
        .down_peers
        .into_iter()
        .map(|d| {
            Ok(core::DownPeer {
                peer: peer_from_proto(d.peer.ok_or("missing down peer")?),
                down_seconds: d.down_seconds,
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let mut r = core::RegionInfo::new(
        meta,
        p.leader.map(peer_from_proto),
        down,
        p.pending_peers.into_iter().map(peer_from_proto).collect(),
    );
    r.set_term(p.term);
    r.set_heartbeat_stats(
        core::RegionHeartbeatStats {
            read_bytes: p.bytes_read,
            read_keys: p.keys_read,
            written_bytes: p.bytes_written,
            written_keys: p.keys_written,
            approximate_size: p.approximate_size,
            approximate_keys: p.approximate_keys,
            approximate_kv_size: p.approximate_kv_size,
            approximate_ia_kv_size: p.approximate_ia_kv_size,
            approximate_columnar_kv_size: p.approximate_columnar_kv_size,
        },
        divisor,
    );
    r.set_query_stats(p.query_stats.map(|q| core::QueryStats {
        gc: q.gc,
        get: q.get,
        scan: q.scan,
        coprocessor: q.coprocessor,
        delete: q.delete,
        delete_range: q.delete_range,
        put: q.put,
        prewrite: q.prewrite,
        acquire_pessimistic_lock: q.acquire_pessimistic_lock,
        commit: q.commit,
        rollback: q.rollback,
    }));
    if let Some(i) = p.interval {
        r.set_report_interval(i.start_timestamp, i.end_timestamp);
    }
    r.set_response_cpu_usage(p.cpu_usage);
    if let Some(c) = p.cpu_stats {
        r.set_read_cpu_usage(c.unified_read);
    }
    if let Some(s) = p.replication_status {
        r.set_replication_status(s.state, s.state_id);
    }
    r.set_bucket_meta(p.bucket_meta.map(|b| core::RegionBucketMeta {
        version: b.version,
        keys: b.keys,
    }));
    Ok(r)
}
