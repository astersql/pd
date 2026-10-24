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

mod stores;
pub use stores::{StoreKindLoads, StoreStatInformer, StoresStats};
#[cfg(test)]
mod stores_test;

mod store_history;
pub use store_history::{
    DEFAULT_HISTORY_SAMPLE_DURATION, DEFAULT_HISTORY_SAMPLE_INTERVAL, HistoryLoads, Loads,
    StoreHistoryLoads,
};
#[cfg(test)]
mod store_history_test;
