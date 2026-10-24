# 存储状态汇总

`StoreStatistics` / `StoreStatisticsMap` 对应 `store_collection.go` 的领域观察与收集，
`StoreMetricSample` 返回 metric family、按 Go 顺序排列的 label values 与 f64 gauge值。
`lib.rs` 显式注册生产模块与独立 `store_collection_test.rs`。

- `observe_store_status` 复用 core 的时间、慢节点与空间判断，宿主传入 Unix 纳秒。
  Preparing/Serving 的 down（>=配置）优先于 unhealthy（>600秒）、
  disconnected（>20秒）、slow 和 up。生命周期与健康分别输出；down 不计低空间，
  Removing 仍计低空间，Removed 只输出十二个状态值；未知状态保留低空间判断。
- `observe` 按 `(type, engine, store)` 更新集群状态/资源样本，按
  `(address, store, type)` 输出分数、计数、字节容量、可选slow-trend与v2窗口样本。
  分数使用 canonical core；引擎复用 core.Engine 规则，包括 compute 归入TiFlash。
  Removed 不更新资源样本，因此同一容器此前的容量/计数保持原值。
- 位置标签使用配置键，空值记 `unknown`，移除节点不追加；保留观察顺序和重复ID。
  `collect` 输出 `(label, name, store)` 值1，重复ID合并为同一gauge键。
  返回所有25项配置值及显式store限额（包括transfer-leader-in），不补隐式限额。
- `StoreStatisticsMap::new` 借用宿主配置视图，`observe` 和 `collect` 转发至其独立
  `stats`。一次完整census通常创建新容器，避免跨轮累积位置成员；此行为与Go调用方一致。
  两个容器的状态、地址/引擎维度、位置与配置完全独立；返回的样本拥有自己的数据。

本任务无Registry、全局指标、协议类型、网络、持久化或后台运行时。注册、清理、
以及hot/region/label指标接线归后续任务28；本文件只输出store观察与collect的领域样本。
TDD与实际Go快照/差分见[任务27证据](../../docs/rust/state/task-27.md)。
