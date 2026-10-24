// Copyright 2026 AsterSQL.

use astersql_pd_core::{PeerStat, RecordPair};

const UNIFIED_READ_POOL_THREAD_PREFIX: &str = "unified-read";

/// Return store read CPU in the original heartbeat units, summing unified-read threads.
///
/// Accumulate as a wrapping u64 before conversion, matching Go even on overflow.
pub fn store_read_cpu_usage(cpu_usages: &[RecordPair]) -> f64 {
    cpu_usages
        .iter()
        .filter(|usage| usage.key.starts_with(UNIFIED_READ_POOL_THREAD_PREFIX))
        .fold(0_u64, |total, usage| total.wrapping_add(usage.value)) as f64
}

/// Return unified-read CPU, or zero when the peer or its CPU stats are missing.
pub fn region_read_cpu_usage(peer_stat: Option<&PeerStat>) -> f64 {
    peer_stat
        .and_then(|peer| peer.cpu_stats.as_ref())
        .map_or(0.0, |cpu| cpu.unified_read as f64)
}
