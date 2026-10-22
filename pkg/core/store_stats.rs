// Copyright 2026 AsterSQL.
//! Raw store heartbeat values. No protocol types or smoothing policy live here.

/// Independent distributed-filesystem counters for one scope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DFSStats {
    pub written_bytes: u64,
    pub write_requests: u64,
}

/// Owned raw heartbeat statistics. Values are preserved without validation,
/// including inconsistent capacity/available reports and unsigned maxima.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreStats {
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
    /// Read the current raw report; a new store starts with all-zero values.
    pub fn store_stats(&self) -> &StoreStats {
        &self.raw_stats
    }

    /// Replace the entire report, including fields reset to zero.
    pub fn update_raw_stats(&mut self, stats: StoreStats) {
        if let Some(average) = &mut self.avg_available {
            average.add(stats.available as f64);
        }
        self.raw_stats = stats;
    }

    pub fn capacity(&self) -> u64 {
        self.raw_stats.capacity
    }

    pub fn available(&self) -> u64 {
        self.raw_stats.available
    }

    pub fn used_size(&self) -> u64 {
        self.raw_stats.used_size
    }

    pub fn is_busy(&self) -> bool {
        self.raw_stats.is_busy
    }

    pub fn sending_snap_count(&self) -> u32 {
        self.raw_stats.sending_snap_count
    }

    pub fn receiving_snap_count(&self) -> u32 {
        self.raw_stats.receiving_snap_count
    }

    pub fn bytes_written(&self) -> u64 {
        self.raw_stats.bytes_written
    }

    pub fn bytes_read(&self) -> u64 {
        self.raw_stats.bytes_read
    }

    pub fn keys_written(&self) -> u64 {
        self.raw_stats.keys_written
    }

    pub fn keys_read(&self) -> u64 {
        self.raw_stats.keys_read
    }
}
