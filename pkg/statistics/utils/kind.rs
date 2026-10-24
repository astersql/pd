// Copyright 2026 AsterSQL.

use std::{collections::BTreeMap, fmt};

pub const BYTE_PRIORITY: &str = "byte";
pub const KEY_PRIORITY: &str = "key";
pub const QUERY_PRIORITY: &str = "query";
pub const CPU_PRIORITY: &str = "cpu";
pub const BYTE_DIM: usize = 0;
pub const KEY_DIM: usize = 1;
pub const QUERY_DIM: usize = 2;
pub const CPU_DIM: usize = 3;
pub const DIM_LEN: usize = 4;
pub const REGION_STAT_COUNT: usize = 8;
pub const STORE_LOAD_COUNT: usize = 12;
pub const RW_TYPE_LEN: usize = 2;
pub const ACTION_TYPE_LEN: usize = 3;

/// Heartbeat intervals in seconds, matching the Go reporting units.
pub const REGION_HEARTBEAT_REPORT_INTERVAL: u64 = 60;
pub const STORE_HEARTBEAT_REPORT_INTERVAL: u64 = 10;
pub const HOT_REGION_ANTI_COUNT: u64 = 2;
pub const DEFAULT_AOT_SIZE: usize = 1;
pub const DEFAULT_WRITE_MF_SIZE: usize = 5;
pub const DEFAULT_READ_MF_SIZE: usize = 5;

/// Defaults indexed by RegionStatKind; byte thresholds use binary KiB.
pub const MIN_HOT_THRESHOLDS: [f64; REGION_STAT_COUNT] =
    [8192.0, 128.0, 128.0, 1024.0, 32.0, 32.0, 1.0, 1.0];

/// Unknown, empty and case-mismatched names fall back to the byte dimension.
pub fn string_to_dim(name: &str) -> usize {
    match name {
        KEY_PRIORITY => KEY_DIM,
        QUERY_PRIORITY => QUERY_DIM,
        CPU_PRIORITY => CPU_DIM,
        _ => BYTE_DIM,
    }
}

/// Unknown dimensions retain the empty Go label rather than a new error string.
pub fn dim_to_string(dim: i32) -> &'static str {
    match dim {
        0 => BYTE_PRIORITY,
        1 => KEY_PRIORITY,
        2 => QUERY_PRIORITY,
        3 => CPU_PRIORITY,
        _ => "",
    }
}

/// Open integer kind preserving Go unknown-value behavior.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct RegionStatKind(i32);

impl RegionStatKind {
    pub const READ_BYTES: Self = Self(0);
    pub const READ_KEYS: Self = Self(1);
    pub const READ_QUERY_NUM: Self = Self(2);
    pub const WRITE_BYTES: Self = Self(3);
    pub const WRITE_KEYS: Self = Self(4);
    pub const WRITE_QUERY_NUM: Self = Self(5);
    pub const READ_CPU: Self = Self(6);
    pub const WRITE_CPU: Self = Self(7);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}
impl fmt::Display for RegionStatKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::READ_BYTES => "read_bytes",
            Self::READ_KEYS => "read_keys",
            Self::READ_QUERY_NUM => "read_query",
            Self::WRITE_BYTES => "write_bytes",
            Self::WRITE_KEYS => "write_keys",
            Self::WRITE_QUERY_NUM => "write_query",
            Self::READ_CPU => "read_cpu",
            Self::WRITE_CPU => "write_cpu",
            _ => "unknown RegionStatKind",
        })
    }
}

/// Open integer kind preserving Go unknown-value behavior.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct StoreLoadKind(i32);

impl StoreLoadKind {
    pub const READ_BYTES: Self = Self(0);
    pub const READ_KEYS: Self = Self(1);
    pub const WRITE_BYTES: Self = Self(2);
    pub const WRITE_KEYS: Self = Self(3);
    pub const READ_QUERY: Self = Self(4);
    pub const WRITE_QUERY: Self = Self(5);
    pub const CPU_USAGE: Self = Self(6);
    pub const DISK_READ_RATE: Self = Self(7);
    pub const DISK_WRITE_RATE: Self = Self(8);
    pub const REGIONS_WRITE_BYTES: Self = Self(9);
    pub const REGIONS_WRITE_KEYS: Self = Self(10);
    pub const READ_CPU: Self = Self(11);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}
impl fmt::Display for StoreLoadKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::READ_BYTES => "store_read_bytes",
            Self::READ_KEYS => "store_read_keys",
            Self::WRITE_BYTES => "store_write_bytes",
            Self::WRITE_KEYS => "store_write_keys",
            Self::READ_QUERY => "store_read_query",
            Self::WRITE_QUERY => "store_write_query",
            Self::CPU_USAGE => "store_cpu_usage",
            Self::DISK_READ_RATE => "store_disk_read_rate",
            Self::DISK_WRITE_RATE => "store_disk_write_rate",
            Self::REGIONS_WRITE_BYTES => "store_regions_write_bytes",
            Self::REGIONS_WRITE_KEYS => "store_regions_write_keys",
            Self::READ_CPU => "store_read_cpu",
            _ => "unknown StoreStatKind",
        })
    }
}

/// Open integer kind preserving Go unknown-value behavior.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct SourceKind(i32);

impl SourceKind {
    pub const DIRECT: Self = Self(0);
    pub const INHERIT: Self = Self(1);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}
impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::DIRECT => "direct",
            Self::INHERIT => "inherit",
            _ => "unknown",
        })
    }
}

/// Open integer kind preserving Go unknown-value behavior.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct RWType(i32);

impl RWType {
    pub const WRITE: Self = Self(0);
    pub const READ: Self = Self(1);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}
impl fmt::Display for RWType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::WRITE => "write",
            Self::READ => "read",
            _ => "unimplemented",
        })
    }
}

/// Open integer kind preserving Go unknown-value behavior.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ActionType(i32);

impl ActionType {
    pub const ADD: Self = Self(0);
    pub const REMOVE: Self = Self(1);
    pub const UPDATE: Self = Self(2);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }
    pub const fn as_i32(self) -> i32 {
        self.0
    }
}
impl fmt::Display for ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::ADD => "add",
            Self::REMOVE => "remove",
            Self::UPDATE => "update",
            _ => "unimplemented",
        })
    }
}

const WRITE_REGION_STATS: [RegionStatKind; DIM_LEN] = [
    RegionStatKind::WRITE_BYTES,
    RegionStatKind::WRITE_KEYS,
    RegionStatKind::WRITE_QUERY_NUM,
    RegionStatKind::WRITE_CPU,
];
const READ_REGION_STATS: [RegionStatKind; DIM_LEN] = [
    RegionStatKind::READ_BYTES,
    RegionStatKind::READ_KEYS,
    RegionStatKind::READ_QUERY_NUM,
    RegionStatKind::READ_CPU,
];

impl RWType {
    /// Per-direction indexes in byte, key, query, CPU order.
    pub fn region_stats(self) -> &'static [RegionStatKind] {
        match self {
            Self::WRITE => &WRITE_REGION_STATS,
            Self::READ => &READ_REGION_STATS,
            _ => &[],
        }
    }

    pub fn inverse(self) -> Self {
        if self == Self::WRITE {
            Self::READ
        } else {
            Self::WRITE
        }
    }

    pub fn report_interval(self) -> u64 {
        if self == Self::WRITE {
            REGION_HEARTBEAT_REPORT_INTERVAL
        } else {
            STORE_HEARTBEAT_REPORT_INTERVAL
        }
    }

    pub fn default_anti_count(self) -> u64 {
        if self == Self::WRITE {
            HOT_REGION_ANTI_COUNT
        } else {
            HOT_REGION_ANTI_COUNT
                * (REGION_HEARTBEAT_REPORT_INTERVAL / STORE_HEARTBEAT_REPORT_INTERVAL)
        }
    }

    /// Select cumulative deltas, dividing each by seconds (including zero).
    /// Panics for a selected index absent from delta_loads, as Go does.
    pub fn get_load_rates(self, delta_loads: &[f64], interval: u64) -> [f64; DIM_LEN] {
        let mut loads = [0.0; DIM_LEN];
        for (dim, kind) in self.region_stats().iter().enumerate() {
            loads[dim] = delta_loads[kind.0 as usize] / interval as f64;
        }
        loads
    }

    /// Write only this direction. Slices must cover the indexes actually used.
    /// Rust borrowing requires independent source and destination slices.
    pub fn set_full_load_rates(self, full: &mut [f64], loads: &[f64]) {
        for (dim, kind) in self.region_stats().iter().enumerate() {
            full[kind.0 as usize] = loads[dim];
        }
    }
}

/// Visit read then write, each in byte, key, query, CPU order.
pub fn foreach_region_stats(mut f: impl FnMut(RWType, usize, RegionStatKind)) {
    for rw in [RWType::READ, RWType::WRITE] {
        for (dim, kind) in rw.region_stats().iter().copied().enumerate() {
            f(rw, dim, kind);
        }
    }
}

/// Build one owned label without coupling values to a global registry.
pub fn single_label(key: impl Into<String>, value: impl Into<String>) -> BTreeMap<String, String> {
    BTreeMap::from([(key.into(), value.into())])
}
