# Store rolling statistics

`RollingStoreStats` 对应 `store.go` 中的单节点滚动容器。输入复用 core 的
`StoreStats`、`TimeInterval`、`RecordPair` 和 `QueryStats` 领域值，边界适配层负责
协议转换。没有 protobuf、网络、存储或运行时依赖。调用方通过独占借用更新；共享容器
负责同步，不在这里引入全局状态或后台任务。

`observe` 将 bytes/keys/query 增量与报告间隔加入真实 `TimeMedian`；七个时间维度
使用10秒平均窗口，读写中位数窗口分别取 statistics-utils 默认值。读 CPU 是用量，
先乘 signed `Duration.Seconds()` 再加入时间平均；其近期最大值窗口为18个报告。
总 CPU、磁盘读写和 region 写入速率直接使用 `MedianFilter`，窗口分别为5、3、9。
`observe_regions_stats` 输入已经是每秒写入速率，独立于 StoreHeartbeat。

`set` 对非零 unsigned 秒差将增量转成每秒速率并替换中位数；零间隔整个操作无效。
`TimeMedian::set` 只替换中位数，保留时间平均及瞬时历史，因此新容器 Set 后时间维度
瞬时值仍为零。其他中位数及最大值 Set 后只有一个样本。心跳 Set 不修改 region 写率。
`set_regions_stats` 只替换 region 写率中位数。

时间戳差、转 signed 纳秒和 collect 求和保留 Go wrapping 行为；缺 interval 等于零。
Observe 零间隔不加入时间平均，已满时仍按 TimeMedian 行为采样，同时更新 CPU/磁盘。
`get_load` 返回平滑值，`get_instant_load` 返回时间平均的最后速率或直接中位数的最后
样本，未知 `StoreLoadKind` 均返回零。空容器读零；CPU只统计 unified-read 前缀时复用
已验收的 `store_read_cpu_usage`。

`store_rolling_test.rs` 显式登记于 lib.rs，7项行为测试覆盖既有 Go CPU 时间中位数
oracle、全部12个负载、不同报告间隔、未满/满窗口及淘汰、Set/Observe 混用、零/缺失
间隔、未知kind、uint64汇总溢出及Duration wrapping。新增边界期望来自源码契约，
不冒称额外的 Go 运行快照。证据见[任务10](../../docs/rust/state/task-10.md)。
