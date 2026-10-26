# 热点方案负载投影

`HotSolution` 借用真实 `StoreLoadDetail` / `HotPeerStat`，提供 current、pending、
extreme 与四维 peers-rate 缓存。`calc_peers_rate` 每次清零全部维度，对选择维度使用
真实 peer 的 denoised/rounded `get_load`，减去可选 revert peer。净 rate 非负（含零）
取 source min/destination max，负数反向。pending 为绝对差，不携带 peer count。
缓存未计算及非法维度沿用 Go 的 panic 前提；宿主更换 peer 后必须重算。

`HotLoadProjection` 持有按 region ID 索引的既有 `PendingInfluence` 与
`StoreHistoryLoads`。summary 复用已验证 `calc_pending_influence`，刷新真实 Operator
的 expiration/timeout，成功操作在 zombie 窗口内权重为1，到期为0并回收；失败终态立即
不贡献影响量，但保留到到期。这里没有线性时间衰减。时钟由宿主提供，与 Operator
使用同一时间域。

有效配置默认权重1，小于1的旧值回退1。`HotRegionConfig` 继续负责新配置验证。
每个 source occurrence 扣减 origin、destination 增加 origin，包括源缺失时对目标
的影响；缺失目标不阻止源扣减；同源同目标抵消。多源会重复增加目标影响量，这是 Go
的现有循环语义。空 source 列表不会进入回收分支。宿主每轮传入新的 store summary，
函数不负责清空调用者的旧影响量。历史配置值相同时保留采样，任一值变化时清空历史，
复用 statistics 的非负 `std::time::Duration` API，不新增配置解析或历史存储副本。

本任务仅负载投影，不装配完整 Schedule、rank、split、placement 决策、HTTP 或持久化。
当前 store liveness 对 metrics 的二次检查留在拥有 cluster 的宿主适配边界。
库无线程、私有运行时或外部 I/O，故障验证作用于真实操作取消/替换/超时/过期和缓存前提。

Go 对照为 `hot_region_solution.go`、`hot_region.go` 和 `statistics/store_load.go`。
本任务依用户明确授权以**源码推导 + Rust真实行为/故障验证**验收，未运行 Go oracle，
不宣称运行差分。Go 测试覆盖范围、冻结 fixture、原始日志、来源哈希、Cargo 槽位及
失败/通过证据见 [task-086](../../../docs/rust/schedule-execution/task-086.md)。
