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
