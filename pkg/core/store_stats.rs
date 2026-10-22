// Copyright 2026 AsterSQL.
//! Raw store heartbeat values. No protocol types or smoothing policy live here.

use crate::QueryStats;

/// Independent distributed-filesystem counters for one scope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DFSStats {
    pub written_bytes: u64,
    pub write_requests: u64,
}

/// Owned raw heartbeat statistics. Values are preserved without validation,
/// including inconsistent capacity/available reports and unsigned maxima.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreStats {
    pub store_id: u64,
    pub region_count: u32,
    pub start_time: u32,
    pub applying_snap_count: u32,
    pub interval: Option<TimeInterval>,
    pub cpu_usages: Vec<RecordPair>,
    pub read_io_rates: Vec<RecordPair>,
    pub write_io_rates: Vec<RecordPair>,
    pub op_latencies: Vec<RecordPair>,
    pub peer_stats: Vec<PeerStat>,
    pub query_stats: Option<QueryStats>,
    pub damaged_regions_id: Vec<u64>,
    pub is_apply_busy: bool,
    pub snapshot_stats: Vec<SnapshotStat>,
    pub slow_trend: Option<SlowTrend>,
    pub is_grpc_paused: bool,
    pub total_memory: u64,
    pub used_memory: u64,
    pub dfs: Vec<DfsStatItem>,
    pub is_stopping: bool,

    pub slow_score: u64,
    pub network_slow_scores: std::collections::HashMap<u64, u64>,
    pub capacity: u64,
    pub available: u64,
    pub used_size: u64,
    pub is_busy: bool,
    pub sending_snap_count: u32,
    pub receiving_snap_count: u32,
    pub bytes_written: u64,
    pub bytes_read: u64,
    pub keys_written: u64,
    pub keys_read: u64,
}

impl super::StoreInfo {
    /// Snapshot the shared raw report; a new store starts with all-zero values.
    pub fn store_stats(&self) -> StoreStats {
        self.stats.read().unwrap().raw.clone()
    }

    /// Replace the entire report, including fields reset to zero.
    pub fn update_raw_stats(&mut self, stats: StoreStats) {
        let mut shared = self.stats.write().unwrap();
        if let Some(average) = &mut shared.average {
            average.add(stats.available as f64);
        }
        shared.raw = stats;
    }

    pub fn capacity(&self) -> u64 {
        self.stats.read().unwrap().raw.capacity
    }

    pub fn available(&self) -> u64 {
        self.stats.read().unwrap().raw.available
    }

    pub fn used_size(&self) -> u64 {
        self.stats.read().unwrap().raw.used_size
    }

    pub fn is_busy(&self) -> bool {
        self.stats.read().unwrap().raw.is_busy
    }

    pub fn sending_snap_count(&self) -> u32 {
        self.stats.read().unwrap().raw.sending_snap_count
    }

    pub fn receiving_snap_count(&self) -> u32 {
        self.stats.read().unwrap().raw.receiving_snap_count
    }

    pub fn bytes_written(&self) -> u64 {
        self.stats.read().unwrap().raw.bytes_written
    }

    pub fn bytes_read(&self) -> u64 {
        self.stats.read().unwrap().raw.bytes_read
    }

    pub fn keys_written(&self) -> u64 {
        self.stats.read().unwrap().raw.keys_written
    }

    pub fn keys_read(&self) -> u64 {
        self.stats.read().unwrap().raw.keys_read
    }
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeInterval {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordPair {
    pub key: String,
    pub value: u64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PeerStat {
    pub region_id: u64,
    pub read_keys: u64,
    pub read_bytes: u64,
    pub query_stats: Option<QueryStats>,
    pub written_keys: u64,
    pub written_bytes: u64,
    pub cpu_stats: Option<CpuStats>,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpuStats {
    pub unified_read: u64,
    pub scheduler: u64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DfsStatScope {
    pub is_global: bool,
    pub keyspace_id: u32,
    pub component: String,
    pub keyspace_identities: Vec<KeyspaceIdentity>,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DfsStatItem {
    pub scope: Option<DfsStatScope>,
    pub written_bytes: u64,
    pub write_requests: u64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlowTrend {
    pub cause_value: f64,
    pub cause_rate: f64,
    pub result_value: f64,
    pub result_rate: f64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapshotStat {
    pub region_id: u64,
    pub generate_duration_sec: u64,
    pub send_duration_sec: u64,
    pub total_duration_sec: u64,
    pub transport_size: u64,
}

/// Owned heartbeat report values, preserved without validation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyspaceIdentity {
    pub namespace_id: u32,
    pub keyspace_id: u32,
}
