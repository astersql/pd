// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod cpu;
pub use cpu::{region_read_cpu_usage, store_read_cpu_usage};

#[cfg(test)]
mod cpu_test;

mod store_rolling;
pub use store_rolling::{
    REGIONS_STATS_OBSERVE_INTERVAL, REGIONS_STATS_ROLLING_WINDOWS_SIZE, RollingStoreStats,
};
#[cfg(test)]
mod store_rolling_test;
