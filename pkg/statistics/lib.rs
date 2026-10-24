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

mod load_values;
pub use load_values::{
    Influence, RegionKindLoads, StoreLoad, StoreLoadPred, StoreSummaryInfo, max_load, min_load,
    summary_store_infos,
};
#[cfg(test)]
mod load_values_test;

mod region_totals;
pub use region_totals::{
    GetRegionStatsOption, HotPeerSnapshot, RegionStatInformer, RegionStats, get_region_stats,
    with_store_map_option,
};
#[cfg(test)]
mod region_totals_test;

mod region_classify;
pub use region_classify::{
    PlacementRulesRequired, RegionAnomalyDurations, RegionInfoProvider, RegionStatisticType,
    RegionStatistics,
};
#[cfg(test)]
mod region_classify_test;
