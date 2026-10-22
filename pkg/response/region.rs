// Copyright 2026 AsterSQL.
//! Owned single-region HTTP responses, preserving Go JSON omission rules.
use crate::{MetaPeer, PDPeerStats};
use astersql_pd_core::RegionInfo as CoreRegion;
use serde::Serialize;
fn zero(value: &u64) -> bool {
    *value == 0
}
/// HTTP epoch, whose zero members are omitted like metapb.RegionEpoch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct APIRegionEpoch {
    #[serde(skip_serializing_if = "zero")]
    pub conf_ver: u64,
    #[serde(skip_serializing_if = "zero")]
    pub version: u64,
}
/// Reported replication state; a present UNKNOWN is distinct from no report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplicationStatus {
    pub state: String,
    pub state_id: u64,
}
/// Complete single-region HTTP response.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RegionInfo {
    pub id: u64,
    pub start_key: String,
    pub end_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<APIRegionEpoch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub peers: Vec<MetaPeer>,
    pub leader: MetaPeer,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub down_peers: Vec<PDPeerStats>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pending_peers: Vec<MetaPeer>,
    pub cpu_usage: u64,
    pub written_bytes: u64,
    pub read_bytes: u64,
    pub written_keys: u64,
    pub read_keys: u64,
    pub approximate_size: i64,
    pub approximate_keys: i64,
    pub approximate_kv_size: i64,
    pub approximate_columnar_kv_size: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buckets: Vec<String>,
    #[serde(skip_serializing_if = "zero")]
    pub bucket_version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replication_status: Option<ReplicationStatus>,
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789ABCDEF";
    bytes
        .iter()
        .flat_map(|b| {
            [
                DIGITS[(b >> 4) as usize] as char,
                DIGITS[(b & 15) as usize] as char,
            ]
        })
        .collect()
}
/// Build a fresh response; an absent region has no response.
pub fn new_api_region_info(region: Option<&CoreRegion>) -> Option<RegionInfo> {
    let mut out = RegionInfo::default();
    init_region(region, &mut out)?;
    Some(out)
}
/// Initialize a reusable response. Absent input leaves the destination untouched.
/// As in Go, a zero/missing bucket version retains the previous version.
pub fn init_region<'a>(
    region: Option<&CoreRegion>,
    out: &'a mut RegionInfo,
) -> Option<&'a mut RegionInfo> {
    let r = region?;
    let meta = r.meta();
    let stats = r.stats();
    let bucket = r.buckets();
    let bucket_version = bucket.as_ref().map_or(0, |b| b.version);
    let (state, state_id) = r.replication_status();
    *out = RegionInfo {
        id: meta.id,
        start_key: hex(&meta.key_range.start_key),
        end_key: hex(&meta.key_range.end_key),
        epoch: meta.epoch.map(|e| APIRegionEpoch {
            conf_ver: e.conf_ver,
            version: e.version,
        }),
        peers: meta
            .peers
            .iter()
            .map(|p| MetaPeer::from_peer(Some(p)))
            .collect(),
        leader: MetaPeer::from_peer(r.leader()),
        down_peers: r.down_peers().iter().map(PDPeerStats::from).collect(),
        pending_peers: r
            .pending_peers()
            .iter()
            .map(|p| MetaPeer::from_peer(Some(p)))
            .collect(),
        cpu_usage: r.response_cpu_usage(),
        written_bytes: stats.written_bytes,
        read_bytes: stats.read_bytes,
        written_keys: stats.written_keys,
        read_keys: stats.read_keys,
        approximate_size: stats.approximate_size,
        approximate_keys: stats.approximate_keys,
        approximate_kv_size: stats.approximate_kv_size,
        approximate_columnar_kv_size: stats.approximate_columnar_kv_size,
        buckets: bucket.map_or_else(Vec::new, |b| b.keys.iter().map(|k| hex(k)).collect()),
        bucket_version: if bucket_version > 0 {
            bucket_version
        } else {
            out.bucket_version
        },
        replication_status: r.has_replication_status().then(|| ReplicationStatus {
            state: match state {
                0 => "UNKNOWN".into(),
                1 => "SIMPLE_MAJORITY".into(),
                2 => "INTEGRITY_OVER_LABEL".into(),
                n => n.to_string(),
            },
            state_id,
        }),
    };
    Some(out)
}
/// Serialize one region. Missing input follows Go's repaired default response.
pub fn marshal_region_info_json(region: Option<&CoreRegion>) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&new_api_region_info(region).unwrap_or_default())
}
