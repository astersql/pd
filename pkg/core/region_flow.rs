// Copyright 2026 AsterSQL.
//! Heartbeat normalization over owned domain values; sizes enter in bytes.
use crate::RegionInfo;
/// Raw heartbeat counters, before unit conversion and invalid-flow filtering.
#[derive(Clone, Debug, Default)]
pub struct RegionHeartbeatStats {
    pub read_bytes: u64,
    pub read_keys: u64,
    pub written_bytes: u64,
    pub written_keys: u64,
    pub approximate_size: u64,
    pub approximate_keys: u64,
    pub approximate_kv_size: u64,
    pub approximate_ia_kv_size: u64,
    pub approximate_columnar_kv_size: u64,
}
/// Normalized counters; sizes are MiB and keys retain Go signed conversion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionStats {
    pub read_bytes: u64,
    pub read_keys: u64,
    pub written_bytes: u64,
    pub written_keys: u64,
    pub approximate_size: i64,
    pub approximate_keys: i64,
    pub approximate_kv_size: i64,
    pub approximate_ia_kv_size: i64,
    pub approximate_columnar_kv_size: i64,
    pub flow_round_divisor: u64,
}
impl RegionHeartbeatStats {
    /// Convert sizes and independently discard impossible read/write pairs.
    pub fn normalize(self, divisor: u64) -> RegionStats {
        let valid_read = self.read_bytes < (1 << 50) && self.read_keys < (1 << 50);
        let valid_write = self.written_bytes < (1 << 50) && self.written_keys < (1 << 50);
        RegionStats {
            read_bytes: if valid_read { self.read_bytes } else { 0 },
            read_keys: if valid_read { self.read_keys } else { 0 },
            written_bytes: if valid_write { self.written_bytes } else { 0 },
            written_keys: if valid_write { self.written_keys } else { 0 },
            approximate_size: (self.approximate_size / 1048576)
                .max(u64::from(self.approximate_size > 0)) as i64,
            approximate_keys: self.approximate_keys as i64,
            approximate_kv_size: (self.approximate_kv_size / 1048576) as i64,
            approximate_ia_kv_size: (self.approximate_ia_kv_size / 1048576) as i64,
            approximate_columnar_kv_size: (self.approximate_columnar_kv_size / 1048576) as i64,
            flow_round_divisor: divisor,
        }
    }
}
/// Decimal rounding divisor. Negative digits disable rounding; overflow saturates.
/// Go's out-of-range float conversion is architecture dependent; saturation matches
/// the arm64 baseline and preserves existing huge-digit small-flow behavior.
pub fn flow_round_divisor_by_digit(digit: i64) -> u64 {
    if digit < 0 {
        0
    } else if digit > 19 {
        u64::MAX
    } else {
        10u64.pow(digit as u32)
    }
}
impl RegionStats {
    fn round(&self, bytes: u64) -> u64 {
        let divisor = self.flow_round_divisor;
        if divisor == 0 {
            bytes
        } else {
            (bytes.wrapping_add(divisor / 2) / divisor).wrapping_mul(divisor)
        }
    }
    /// Round read bytes to the nearest divisor, with half ties upward.
    pub fn round_bytes_read(&self) -> u64 {
        self.round(self.read_bytes)
    }
    /// Round written bytes to the nearest divisor, with Go unsigned wrapping.
    pub fn round_bytes_written(&self) -> u64 {
        self.round(self.written_bytes)
    }
}
impl RegionInfo {
    /// Normalize and replace heartbeat statistics without altering membership.
    pub fn set_heartbeat_stats(&mut self, stats: RegionHeartbeatStats, divisor: u64) {
        self.stats = stats.normalize(divisor);
    }
    /// Read the normalized heartbeat snapshot.
    pub fn stats(&self) -> &RegionStats {
        &self.stats
    }
}
