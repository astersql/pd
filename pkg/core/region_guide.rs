// Copyright 2026 AsterSQL.
//! Follow-up decisions matching Go GenerateRegionGuideFunc, without logging effects.
use crate::{Peer, RegionInfo};

/// Storage, cache, sync and retained decisions, accumulated in Go branch order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionGuide {
    pub save_kv: bool,
    pub save_cache: bool,
    pub need_sync: bool,
    pub retained: bool,
}
fn peers_equal<'a>(a: impl Iterator<Item = &'a Peer>, b: impl Iterator<Item = &'a Peer>) -> bool {
    a.map(|p| (p.id, p.store_id))
        .eq(b.map(|p| (p.id, p.store_id)))
}
impl RegionInfo {
    /// Set the heartbeat term. The guide deliberately does not compare terms.
    pub fn set_term(&mut self, term: u64) {
        self.term = term;
    }
    /// Read the reported term.
    pub fn term(&self) -> u64 {
        self.term
    }
    /// Set replication state and state ID; state zero means UNKNOWN.
    pub fn set_replication_status(&mut self, state: i32, state_id: u64) {
        self.replication_status = (state, state_id);
    }
}
/// Decide follow-up work. Staleness validation belongs to the caller.
/// Peer equality compares identity only; down durations and roles are ignored.
pub fn region_guide(region: &RegionInfo, origin: Option<&RegionInfo>) -> RegionGuide {
    let Some(origin) = origin else {
        return RegionGuide {
            save_kv: true,
            save_cache: true,
            retained: true,
            need_sync: false,
        };
    };
    let mut result = RegionGuide::default();
    let r = region.meta.epoch.unwrap_or_default();
    let o = origin.meta.epoch.unwrap_or_default();
    if r.version > o.version || r.conf_ver > o.conf_ver {
        result.save_kv = true;
        result.save_cache = true;
        result.retained = true;
    }
    if region.leader.as_ref().map_or(0, |p| p.id) != origin.leader.as_ref().map_or(0, |p| p.id) {
        result.save_cache = true;
        result.need_sync = true;
    }
    let bucket_version = |r: &RegionInfo| {
        r.bucket_meta
            .as_ref()
            .map(|b| b.version)
            .unwrap_or_else(|| r.report_buckets.as_ref().map_or(0, |b| b.version))
    };
    let new_bucket = bucket_version(region);
    let old_bucket = bucket_version(origin);
    if region.meta.peers.len() != origin.meta.peers.len()
        || new_bucket > old_bucket
        || (new_bucket == 0 && old_bucket > 0)
    {
        result.save_kv = true;
        result.save_cache = true;
        return result;
    }
    let r = &region.stats;
    let o = &origin.stats;
    if r.round_bytes_written() != o.round_bytes_written()
        || r.round_bytes_read() != o.round_bytes_read()
        || r.flow_round_divisor < o.flow_round_divisor
        || !peers_equal(
            region.down_peers.iter().map(|p| &p.peer),
            origin.down_peers.iter().map(|p| &p.peer),
        )
        || !peers_equal(region.pending_peers.iter(), origin.pending_peers.iter())
    {
        result.save_cache = true;
        result.need_sync = true;
        return result;
    }
    if (
        r.approximate_size,
        r.approximate_keys,
        r.approximate_kv_size,
        r.approximate_ia_kv_size,
        r.approximate_columnar_kv_size,
    ) != (
        o.approximate_size,
        o.approximate_keys,
        o.approximate_kv_size,
        o.approximate_ia_kv_size,
        o.approximate_columnar_kv_size,
    ) || (region.replication_status.0 != 0
        && region.replication_status != origin.replication_status)
        || region.meta.is_in_flashback != origin.meta.is_in_flashback
        || region.meta.flashback_start_ts != origin.meta.flashback_start_ts
    {
        result.save_cache = true;
    }
    result
}
