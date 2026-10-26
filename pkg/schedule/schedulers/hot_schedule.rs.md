# 热点读写调度接线

`HotScheduler::{new,schedule,dispatch}` 使用四个 canonical `HotSolver`，资源顺序
保持 Go 的 writePeer/writeLeader/readPeer/readLeader。`HotRankInputs` 为该轮
各资源真实统计投影的极值和步长；配置通过任务85验证与能力降级。实例对应这些
统计/配置快照的搜索轮次，快照更新后由宿主重建；重复调用保留各资源的反向搜索状态。

调用方先完成统计/cache/placement 准入与准备，再传入四类不可变候选；禁用读写
检查在 dispatch 内，不允许调用方因此跳过准备。随机数由宿主提供，schedule 按
Go 的取模选择给定资源集，不安装 RNG、runtime、全局指标或 HTTP。

读方向无论抽到 leader 还是 peer 都运行两个求解器。通过
`HotSolver::prefers_result` 使用 leader 的真实排序公式比较两者，平局先 peer。
只有一个有解则只提供该解；两者有解则保留排序后的可执行 offers，以便后继
pending 生命周期任务95在首选原子准入失败时使用备选。当前 API 返回候选 operator，
不提交 operator、不声称已经登记 pending。写方向只运行对应单求解器。
分配 ID 失败通过 BuilderError 返回，某条读路径失败不丢掉另一条成功方案。

`is_schedule_allowed` 严格比较 hot operator 数与限额（等于即拒绝）；宿主的控制器
调用此门控。`next_interval` 忽略传入旧值，总是将1秒增长为1.3秒。
`Forbidden` 与 `NoSolution` 为领域诊断；错误另保留 typed builder errors。
空资源集返回 `EmptyResources`。没有新增存储写入，故不引入 MetaStore 模拟。

任务93的 `HotSplit` 已独立验证；本模块只承担读写资源路由与方案比较，不重复
实现桶拆分算法，也不扩展真实集群/HTTP 装配。领域 admission、bucket split 和
pending APIs 的宿主全链装配仍须在后续 facade/集群接线中验收。

来源为只读 `hot_region.go` 的 dispatch、balanceHotReadRegions、
balanceHotWritePeers/Leaders、IsScheduleAllowed、GetNextInterval、resourceType，
以及 rank_v1/v2.betterThan。按用户授权使用源码推导，未取得或声称 Go 运行差分。
测试及错误接线红灯见 [任务94证据](../../../docs/rust/schedule-execution/task-094.md)。
