# 慢网络驱逐恢复状态机

`EvictNetworkScheduler` 复用 canonical `BasicCluster`/`StoreInfo`、task-080 网络判定、
`StorageEndpoint` 和 `EvictLeaderScheduler`。显式纳秒时钟由宿主传入；不创建 runtime。
生产实现 `evict_network.rs`；测试独立注册于 `task_081_test.rs`。

配置使用 Go 的同一个 `evict-slow-store-scheduler` JSON envelope，flatten 的
`EvictDiskConfig` 保留 recovery-duration、evict-stores、batch。网络状态为 enabled、
paused、evicted；等待候选和恢复开始时间不持久化。`update_policy` 只更新数值策略，
不清空磁盘恢复状态。宿主须串行协调磁盘/网络配置所有权，避免从过时快照互相覆盖。
本领域模块只负责网络生命周期及网络操作器；磁盘生命周期仍由 task-079 负责。

`observe` 按恢复→激活→检测→驱逐执行。恢复平均值先去掉已跟踪/已知 removed 目标，
u64 求和按 Go wrapping，整数除法截断；均值<=1开始计时，>1重置。即使 recovery=0，
也需要下一轮才能删除。tracked missing/Removed/Removing 先删除；Go 检测仍包含
Removing，因此其冻结慢评分可能同轮重新捕获，测试明确保留这一行为。
暂停容量判定使用检测阶段开始时的 paused 快照；不擅自修正 Go 的同轮多候选行为。
BTreeMap/排序仅固定 Go 未指定的 map 遍历/激活选择顺序；多候选不承诺 Go 随机赢家。

暂停后持久化失败则回滚引用；累计 network triggers>=6、未恢复且已暂停才驱逐。
驱逐持久化失败回滚自身 slow-store 引用。RAII 同时覆盖 Rust 异步取消。
删除先持久化，失败保留集合、时钟、pause/evict 引用便于重试。
禁用时对 paused 删除后清空运行恢复表，保留 Go 即使写失败也清空表的行为。
其它 owner 的引用不会被网络清理释放。

`prepare` 从 paused 重建 runtime map，恢复 pause/evict 引用，返回全部 typed store
错误。`clean` 仅逐一清理 paused；`reload` 调整旧/新 disk+network pause 并保留
runtime recovery map，evicted 引用按 Go 不在 reload 重建。`update` 验证 batch，
写失败恢复数值策略/集合，enable flag 和 Go 一样保留请求的新值。
存储和 JSON 错误通过 `EvictNetworkError` 向宿主传播；其它节点仍可继续推进。
`schedule` 调用真实 leader 驱逐组件，返回 URGENT/LEADER 操作器及状态推进错误。

依据为只读 `evict_slow_store.go` 与相邻测试，包括 TestNetworkSlowStore、
ReachLimit、SwitchEnableToDisable、TestAddNetworkSlowStoreLockedSkipsGaugeOnPersistFailure、
TestNetworkNotConflictWithOtherScheduler、TestCalculateAvgScore。
按用户明确授权使用源码推导和真实 Rust 行为/故障验证，未运行需 failpoint 的 Go suite，
无 Go 运行 oracle 或运行差分主张。HTTP 和指标适配不在本领域模块中实现。

完整命令、案例、故障、provenance 和槽位证据见
[task-081](../../../docs/rust/schedule-execution/task-081.md)。

任务141新增 `update_with_disk`：同步当前 disk 状态再复用 update 事务，成功后
同步 disk 数值策略；恢复表、引用和时钟保持。服务适配层独占借用两个 owner。
