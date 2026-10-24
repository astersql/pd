// Copyright 2026 AsterSQL.
//! Query counters and reported region loads, independent of protobuf.
use crate::RegionInfo;
/// Owned query counters. Arithmetic follows Go uint64 wrapping.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryStats {
    pub gc: u64,
    pub get: u64,
    pub scan: u64,
    pub coprocessor: u64,
    pub delete: u64,
    pub delete_range: u64,
    pub put: u64,
    pub prewrite: u64,
    pub acquire_pessimistic_lock: u64,
    pub commit: u64,
    pub rollback: u64,
}
impl QueryStats {
    /// Add all counters, including GC, into this owned snapshot.
    pub fn merge(&mut self, other: &Self) {
        self.gc = self.gc.wrapping_add(other.gc);
        self.get = self.get.wrapping_add(other.get);
        self.scan = self.scan.wrapping_add(other.scan);
        self.coprocessor = self.coprocessor.wrapping_add(other.coprocessor);
        self.delete = self.delete.wrapping_add(other.delete);
        self.delete_range = self.delete_range.wrapping_add(other.delete_range);
        self.put = self.put.wrapping_add(other.put);
        self.prewrite = self.prewrite.wrapping_add(other.prewrite);
        self.acquire_pessimistic_lock = self
            .acquire_pessimistic_lock
            .wrapping_add(other.acquire_pessimistic_lock);
        self.commit = self.commit.wrapping_add(other.commit);
        self.rollback = self.rollback.wrapping_add(other.rollback);
    }
    /// Count read operations, excluding GC.
    pub fn read_query_num(&self) -> u64 {
        self.get
            .wrapping_add(self.scan)
            .wrapping_add(self.coprocessor)
    }
    /// Count raw and transactional write operations.
    pub fn write_query_num(&self) -> u64 {
        self.put
            .wrapping_add(self.delete)
            .wrapping_add(self.delete_range)
            .wrapping_add(self.acquire_pessimistic_lock)
            .wrapping_add(self.commit)
            .wrapping_add(self.prewrite)
            .wrapping_add(self.rollback)
    }
    /// Clear read counters, preserving writes and GC.
    pub fn reset_read(&mut self) {
        self.get = 0;
        self.scan = 0;
        self.coprocessor = 0;
    }
    /// Clear write counters, preserving reads and GC.
    pub fn reset_write(&mut self) {
        self.put = 0;
        self.delete = 0;
        self.delete_range = 0;
        self.acquire_pessimistic_lock = 0;
        self.commit = 0;
        self.prewrite = 0;
        self.rollback = 0;
    }
}
impl RegionInfo {
    /// Replace the owned query snapshot; absence reports zero queries.
    pub fn set_query_stats(&mut self, stats: Option<QueryStats>) {
        self.query_stats = stats;
    }
    /// Merge an optional snapshot into existing statistics.
    pub fn add_query_stats(&mut self, stats: Option<QueryStats>) {
        self.query_stats
            .get_or_insert_default()
            .merge(&stats.unwrap_or_default());
    }
    /// Return read query count.
    pub fn read_query_num(&self) -> u64 {
        self.query_stats
            .as_ref()
            .map_or(0, QueryStats::read_query_num)
    }
    /// Return write query count.
    pub fn write_query_num(&self) -> u64 {
        self.query_stats
            .as_ref()
            .map_or(0, QueryStats::write_query_num)
    }
    /// Set report timestamps in seconds, preserving unsigned subtraction semantics.
    pub fn set_report_interval(&mut self, start: u64, end: u64) {
        self.report_interval = (start, end);
    }
    /// Return the heartbeat's start and end timestamps in whole seconds.
    pub fn report_interval(&self) -> (u64, u64) {
        self.report_interval
    }
    /// Set the unified-read CPU counter reported by the heartbeat.
    pub fn set_read_cpu_usage(&mut self, usage: u64) {
        self.read_cpu_usage = usage;
    }
    /// Return bytes and keys per second for report intervals from 3 through 300 seconds.
    pub fn write_rate(&self) -> (f64, f64) {
        let interval = self.report_interval.1.wrapping_sub(self.report_interval.0);
        if (3..=300).contains(&interval) {
            (
                self.stats.written_bytes as f64 / interval as f64,
                self.stats.written_keys as f64 / interval as f64,
            )
        } else {
            (0., 0.)
        }
    }
    /// Return read/write bytes, keys, queries and CPU in Go load order.
    pub fn loads(&self) -> [f64; 8] {
        [
            self.stats.read_bytes as f64,
            self.stats.read_keys as f64,
            self.read_query_num() as f64,
            self.stats.written_bytes as f64,
            self.stats.written_keys as f64,
            self.write_query_num() as f64,
            self.read_cpu_usage as f64,
            0.,
        ]
    }
    /// Return only write counters, with all other load dimensions zeroed.
    pub fn write_loads(&self) -> [f64; 8] {
        [
            0.,
            0.,
            0.,
            self.stats.written_bytes as f64,
            self.stats.written_keys as f64,
            self.write_query_num() as f64,
            0.,
            0.,
        ]
    }
}
