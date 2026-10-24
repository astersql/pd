// Copyright 2026 AsterSQL.
//! Store heartbeat smoothing over report intervals and independent region rates.

use astersql_pd_core::{RecordPair, StoreStats};
use astersql_pd_movingaverage::{MaxFilter, MedianFilter, TimeMedian};
use astersql_pd_statistics_utils::{
    DEFAULT_AOT_SIZE, DEFAULT_READ_MF_SIZE, DEFAULT_WRITE_MF_SIZE, STORE_HEARTBEAT_REPORT_INTERVAL,
    STORE_LOAD_COUNT, StoreLoadKind,
};

use crate::store_read_cpu_usage;

const NANOS_PER_SECOND: i64 = 1_000_000_000;
/// Region-tree observation period in seconds.
pub const REGIONS_STATS_OBSERVE_INTERVAL: u64 = 30;
/// Median window for region-tree write rates.
pub const REGIONS_STATS_ROLLING_WINDOWS_SIZE: usize = 9;

#[derive(Clone, Debug)]
enum Filter {
    Time(TimeMedian),
    Median(MedianFilter),
}

impl Filter {
    fn get(&self) -> f64 {
        match self {
            Self::Time(filter) => filter.get(),
            Self::Median(filter) => filter.get(),
        }
    }
    fn instantaneous(&self) -> f64 {
        match self {
            Self::Time(filter) => filter.get_instantaneous(),
            Self::Median(filter) => filter.get_instantaneous(),
        }
    }
    fn add(&mut self, value: f64, interval: i64) {
        match self {
            Self::Time(filter) => filter.add(value, interval),
            Self::Median(filter) => filter.add(value),
        }
    }
    fn set(&mut self, value: f64) {
        match self {
            Self::Time(filter) => filter.set(value),
            Self::Median(filter) => filter.set(value),
        }
    }
}

/// Recent store load history using the same windows and units as Go store.go.
/// Mutations require exclusive access; a sharing container supplies synchronization.
#[derive(Clone, Debug)]
pub struct RollingStoreStats {
    filters: [Filter; STORE_LOAD_COUNT],
    read_cpu_recent_max: MaxFilter,
}

impl Default for RollingStoreStats {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingStoreStats {
    /// Creates empty time, median and recent-maximum filters.
    pub fn new() -> Self {
        let filters = std::array::from_fn(|i| {
            let kind = StoreLoadKind::from_i32(i as i32);
            match kind {
                StoreLoadKind::CPU_USAGE => Filter::Median(MedianFilter::new(5)),
                StoreLoadKind::DISK_READ_RATE | StoreLoadKind::DISK_WRITE_RATE => {
                    Filter::Median(MedianFilter::new(3))
                }
                StoreLoadKind::REGIONS_WRITE_BYTES | StoreLoadKind::REGIONS_WRITE_KEYS => {
                    Filter::Median(MedianFilter::new(
                        REGIONS_STATS_ROLLING_WINDOWS_SIZE as isize,
                    ))
                }
                _ => {
                    let size = match kind {
                        StoreLoadKind::WRITE_BYTES
                        | StoreLoadKind::WRITE_KEYS
                        | StoreLoadKind::WRITE_QUERY => DEFAULT_WRITE_MF_SIZE,
                        _ => DEFAULT_READ_MF_SIZE,
                    };
                    Filter::Time(TimeMedian::new(
                        DEFAULT_AOT_SIZE as isize,
                        size as isize,
                        (STORE_HEARTBEAT_REPORT_INTERVAL as i64).wrapping_mul(NANOS_PER_SECOND),
                    ))
                }
            }
        });
        Self {
            filters,
            read_cpu_recent_max: MaxFilter::new((180 / STORE_HEARTBEAT_REPORT_INTERVAL) as isize),
        }
    }

    /// Adds a heartbeat. CPU is a usage rate; bytes, keys and queries are deltas.
    /// Zero intervals still update instantaneous medians and the recent CPU maximum.
    pub fn observe(&mut self, stats: &StoreStats) {
        let seconds = interval_seconds(stats);
        let interval = (seconds as i64).wrapping_mul(NANOS_PER_SECOND);
        // Match time.Duration.Seconds, including nanosecond wrapping and rounding.
        let duration_seconds = (interval / NANOS_PER_SECOND) as f64
            + (interval % NANOS_PER_SECOND) as f64 / NANOS_PER_SECOND as f64;
        let read_cpu = store_read_cpu_usage(&stats.cpu_usages);
        for (kind, delta) in counters(stats) {
            self.filters[kind.as_i32() as usize].add(delta, interval);
        }
        self.filters[StoreLoadKind::READ_CPU.as_i32() as usize]
            .add(read_cpu * duration_seconds, interval);
        self.read_cpu_recent_max.add(read_cpu);
        for (kind, value) in instantaneous_values(stats) {
            self.filters[kind.as_i32() as usize].add(value, interval);
        }
    }

    /// Replaces smoothed loads for a nonzero unsigned report interval.
    /// Time-average history and its instantaneous values survive this operation.
    pub fn set(&mut self, stats: &StoreStats) {
        let seconds = interval_seconds(stats);
        if seconds == 0 {
            return;
        }
        for (kind, delta) in counters(stats) {
            self.filters[kind.as_i32() as usize].set(delta / seconds as f64);
        }
        let read_cpu = store_read_cpu_usage(&stats.cpu_usages);
        self.filters[StoreLoadKind::READ_CPU.as_i32() as usize].set(read_cpu);
        self.read_cpu_recent_max.set(read_cpu);
        for (kind, value) in instantaneous_values(stats) {
            self.filters[kind.as_i32() as usize].set(value);
        }
    }

    /// Adds region-tree write rates, already measured per second.
    pub fn observe_regions_stats(&mut self, write_bytes_rate: f64, write_keys_rate: f64) {
        self.filters[StoreLoadKind::REGIONS_WRITE_BYTES.as_i32() as usize].add(write_bytes_rate, 0);
        self.filters[StoreLoadKind::REGIONS_WRITE_KEYS.as_i32() as usize].add(write_keys_rate, 0);
    }

    /// Replaces region-tree rate medians with a single sample.
    pub fn set_regions_stats(&mut self, write_bytes_rate: f64, write_keys_rate: f64) {
        self.filters[StoreLoadKind::REGIONS_WRITE_BYTES.as_i32() as usize].set(write_bytes_rate);
        self.filters[StoreLoadKind::REGIONS_WRITE_KEYS.as_i32() as usize].set(write_keys_rate);
    }

    /// Returns the smoothed load, or zero for an unknown kind.
    pub fn get_load(&self, kind: StoreLoadKind) -> f64 {
        self.filters
            .get(kind.as_i32() as usize)
            .map_or(0., Filter::get)
    }

    /// Returns the last sample's rate, or zero for an unknown kind.
    pub fn get_instant_load(&self, kind: StoreLoadKind) -> f64 {
        self.filters
            .get(kind.as_i32() as usize)
            .map_or(0., Filter::instantaneous)
    }

    /// Returns the maximum read CPU usage among the last 18 observed reports.
    pub fn get_read_cpu_recent_max(&self) -> f64 {
        self.read_cpu_recent_max.get()
    }
}

fn interval_seconds(stats: &StoreStats) -> u64 {
    stats.interval.as_ref().map_or(0, |interval| {
        interval
            .end_timestamp
            .wrapping_sub(interval.start_timestamp)
    })
}

fn counters(stats: &StoreStats) -> [(StoreLoadKind, f64); 6] {
    let read_query = stats.query_stats.as_ref().map_or(0, |q| q.read_query_num());
    let write_query = stats
        .query_stats
        .as_ref()
        .map_or(0, |q| q.write_query_num());
    [
        (StoreLoadKind::WRITE_BYTES, stats.bytes_written as f64),
        (StoreLoadKind::WRITE_KEYS, stats.keys_written as f64),
        (StoreLoadKind::WRITE_QUERY, write_query as f64),
        (StoreLoadKind::READ_BYTES, stats.bytes_read as f64),
        (StoreLoadKind::READ_KEYS, stats.keys_read as f64),
        (StoreLoadKind::READ_QUERY, read_query as f64),
    ]
}

fn collect(records: &[RecordPair]) -> f64 {
    records
        .iter()
        .fold(0_u64, |total, record| total.wrapping_add(record.value)) as f64
}

fn instantaneous_values(stats: &StoreStats) -> [(StoreLoadKind, f64); 3] {
    [
        (StoreLoadKind::CPU_USAGE, collect(&stats.cpu_usages)),
        (StoreLoadKind::DISK_READ_RATE, collect(&stats.read_io_rates)),
        (
            StoreLoadKind::DISK_WRITE_RATE,
            collect(&stats.write_io_rates),
        ),
    ]
}
