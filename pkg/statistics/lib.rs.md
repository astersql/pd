# Statistics canonical crate

`astersql-pd-statistics` 是统计包唯一 canonical crate，默认功能集合，禁止 unsafe。
当前公开[CPU归一化](cpu.rs.md)的 store_read_cpu_usage 与 region_read_cpu_usage，
依赖 canonical core 领域值，不依赖 protobuf、传输或调度决策。
`cpu_test` 在 cfg(test) 下显式登记，生产测试位于独立 cpu_test.rs。
后续统计行为在本 crate 原位扩展，不创建平行实现。

已加入[存储节点滚动统计](store_rolling.rs.md)：公开 RollingStoreStats、
REGIONS_STATS_OBSERVE_INTERVAL 和 REGIONS_STATS_ROLLING_WINDOWS_SIZE。
复用 core、movingaverage 与 statistics-utils 领域能力；store_rolling_test 显式登记。

已加入[存储统计容器生命周期](stores.rs.md)：StoresStats、StoreKindLoads 与
StoreStatInformer，所有读取返回独立快照，stores_test 显式登记。

已加入[负载历史采样与回收](store_history.rs.md)：StoreHistoryLoads、Loads、
HistoryLoads 及默认采样配置；store_history_test 显式登记。

已加入[负载预测值代数](load_values.rs.md)：Influence、RegionKindLoads、
StoreSummaryInfo、summary_store_infos、StoreLoad、StoreLoadPred、min_load和max_load；
load_values_test显式登记。复用Loads、HistoryLoads与StoreKindLoads，不重复类型实现。

已加入[区域汇总与过滤](region_totals.rs.md)：RegionStats、get_region_stats、
RegionStatInformer、HotPeerSnapshot、GetRegionStatsOption与with_store_map_option；
region_totals_test显式登记。热点领域输入是已求值快照，完整窗口在后续任务扩展。

已加入[区域异常分类索引](region_classify.rs.md)：RegionStatistics、RegionStatisticType、
RegionInfoProvider、RegionAnomalyDurations与PlacementRulesRequired；region_classify_test显式登记。
复用canonical CheckerConfigProvider，placement禁用时更新与清除10类异常；宿主传入时钟并消费duration样本。

已加入[规则驱动的分类入口](region_placement.rs.md)：RegionStatistics::observe_with_placement，
复用真实RuleManager查询和同一异常索引；region_placement_test显式登记。未初始化返回None且不改变索引。

已加入[标签隔离状态统计](label_statistics.rs.md)：get_region_label_isolation、
is_region_label_isolation_satisfied与LabelStatistics；label_statistics_test显式登记。
复用core标签比较，覆盖Unicode/缺失值、重复观察与延迟defunct清理；宿主消费独立计数快照。

已加入[热点单副本窗口](hot_peer.rs.md)：DimStat、HotPeerStat和显式hot_peer_test。
复用真实TimeMedian和utils动作/方向/TopN，宿主传入时间；支持nil滚动负载回退、CPU last-average、
窗口克隆隔离及Go兼容去噪快照。完整热点缓存生命周期由后续任务扩展。
