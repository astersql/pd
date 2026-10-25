# 区域巡逻生命周期

生产实现：[patrol.rs](patrol.rs)；验证：[task_056_test.rs](task_056_test.rs)、
[任务证据](../../../docs/rust/schedule-execution/task-056.md)。

`Patrol::run` 在宿主 Tokio task/LocalSet 上驱动生命周期，独占当前巡逻实例。
`PatrolHost` 提供宿主持有的配置、canonical region registry、dispatcher、split-scatter
和统计状态访问；它不创建镜像状态面。`CheckerDispatcher::check_patrol_region` 是实际
检查接线：priority 用 `check_region`，跳过相关 merge pair，检查真实 store budget 并入队；
pending/scan 直接复用 `try_add_operators` 的 pending 转移。priority 路径不额外删除 pending。

宿主实现 `priority_ids` 时用 canonical `PriorityInspector::get_priority_regions`，保留其重试时钟；
`pending_ids` 用共享 TTL cache；`scan` 用 `RegionsInfo::scan_regions` 且遵守 limit。
`check` 根据 `PatrolSource` 调用上述分派接口，错误/diagnostic 应按宿主策略收集，
需要结束巡逻的 I/O 故障返回关联 Error 类型。`dispatch_split_scatter` 和
`clear_split_scatter` 调用既有 retry controller；`update_label_stats` 更新已有 LabelStatistics。
宿主负责原有 TTL cache 自身的 shutdown；巡逻不接管其它组件的生命周期。

每个 tick 按配置调整 ticker、取消并等待旧 workers 后启动新 workers；允许 0 个 workers。
暂停调度时丢弃 backlog，有 backlog 时跳过新扫描；否则依次 priority、pending、split-scatter、
scan、label stats。扫描游标使用最后一个 region 的 end key，空扫描只重置游标，
非空且 end key 为空才刷新轮次耗时与 scan limit。limit 为 `(count / 1024).clamp(128,8192)`。
channel 容量固定为 8192；status watch 暴露配置、游标、limit、tick/round 与耗时。
无进程退出、信号/全局 subscriber 或私有 Tokio runtime。

worker futures 在宿主执行器内并发轮询，不要求借用中的 checker/rule state 为 Send；
宿主可以串行持有领域状态锁，但不得在等待扫描 channel 时持有它。
缩容、停止和错误路径都取消 idle workers，等待所有已经开始的 `check` 完成，再清理 split-scatter。
不将已开始的领域检查直接丢弃。宿主应取消 token 并 await `run`；直接 drop future
属于非优雅中断，会 drop 其所有局部 worker futures。零 interval 返回 `PatrolError::InvalidInterval`，
宿主故障返回 `PatrolError::Host(E)`；首个错误保留，剩余 workers 仍等待结束。

行为来自只读 `checker_controller.go` 的源码契约推导；未运行 Go oracle，未声称运行差分。
测试冻结 count、phase order 和 lifecycle 场景，以真实 core、dispatcher、placement/affinity、
MemoryStore、operator admission、label statistics、split-scatter 验证状态与操作。
仅宿主时间、外部检查阻塞/故障以及聚合 region count 快照受控。

任务58增加 `set_metrics(CheckerMetrics)`：取得列表时发布 queue gauge，按 Go
controller 的阶段边界记录 duration，完整轮次发布耗时，joined shutdown 后归零。
队列 gauge 保留最后采样值；halted tick 不更新它们。见 [指标说明](checker_metrics.rs.md)。
