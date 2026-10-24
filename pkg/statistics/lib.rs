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

mod region_placement;
#[cfg(test)]
mod region_placement_test;

mod label_statistics;
pub use label_statistics::{
    LabelStatistics, get_region_label_isolation, is_region_label_isolation_satisfied,
};
#[cfg(test)]
mod label_statistics_test;

mod hot_peer;
pub use hot_peer::{DimStat, HotPeerStat};
#[cfg(test)]
mod hot_peer_test;

mod hot_peer_update;
pub use hot_peer_update::HotPeerCache;
#[cfg(test)]
mod hot_peer_update_test;

mod hot_threshold;
#[cfg(test)]
mod hot_threshold_test;

mod hot_inherit;
#[cfg(test)]
mod hot_inherit_test;

mod hot_expire;
#[cfg(test)]
mod hot_expire_test;

mod hot_worker;
pub use hot_worker::{
    HotCache, HotTask, HotTaskError, HotTaskOutput, HotTaskReceipt, new_region_flow_tasks,
};
#[cfg(test)]
mod hot_worker_test;

mod engine_load;
pub use engine_load::{StoreLoadDetail, StoreLoadSummary, summary_stores_load};
#[cfg(test)]
mod engine_load_test;

mod hot_projection;
pub use hot_projection::{
    HotPeerStatShow, HotPeersStat, StoreHotPeersInfos, StoreHotPeersStat, collect_hot_peer_infos,
    get_hot_status,
};
#[cfg(test)]
mod hot_projection_test;

mod slow_stat;
pub use slow_stat::{SlowStat, SlowStoresStats};
#[cfg(test)]
mod slow_stat_test;

mod store_collection;
pub use store_collection::{StoreMetricSample, StoreStatistics, StoreStatisticsMap};
#[cfg(test)]
mod store_collection_test;
