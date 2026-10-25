# 检查器观测指标

[checker_metrics.rs](checker_metrics.rs) 在 canonical checker crate 提供
`CheckerMetrics::register(&Registry)`、`CheckerMetricKind` 和有界 `event` API。
来源为只读 [metrics.go](metrics.go) 和 [checker_controller.go](checker_controller.go)。
依据用户授权采用源码契约推导及 Rust 行为验证，没有 Go 运行或运行差分证据。

保留 Go 的9类 family name/help/type、label、113个固定事件和默认 histogram buckets。
指标名称使用历史的 `joint_state_checker` 等标签，与配置名称不同。
未知事件返回 Prometheus 错误，不创建任意 series。初始化时预缓存句柄，通过 Arc 共享，
检查热路径不调用 WithLabelValues。Registry 碰撞返回原错误并回滚本次注册，保留宿主已有
collector。Registry 持有 collector，drop 句柄不会注销，销毁整个宿主 Registry 才结束其生命周期。

宿主在启动前分别调用 `CheckerDispatcher::set_metrics`、`Patrol::set_metrics`、
`SplitScatterRetryController::set_metrics`，传入同一个 CheckerMetrics 的 clone。
每份 Registry 对应一个 controller，无进程全局注册、私有 runtime 或信号处理。

controller 的每个实际进入阶段记录 checker duration，包含暂停、no-op、限额跳过；
首个 operator 返回后，不为未进入的阶段记录样本。joint 的 check/paused/factory failure/
new-operator/transfer-leader，split 的 check/paused，learner 的 paused，rule 的 check/paused/
no-leader/cache-hit，以及 replica、affinity、merge 的调用/暂停事件随真实分派更新。
replica、merge 成功结果记录 new-operator。其它领域细分事件可由所属领域通过有界 event API
上报；113个事件的 schema 测试不代表每条业务分支都经过运行验收。

巡逻按 Go 阶段边界记录六类 phase duration；空扫描不记录 label stats。
priority/pending gauge 在取得列表快照时更新，channel gauge 在未 halted 的 tick 采样。
有 backlog 时只记录 wait_for_channel，完整轮次更新 patrol_regions_time。
取消和错误退出等待在途 workers，再清理 split-scatter、归零轮次耗时。
队列 gauge 保留最后采样值，符合 Go；停止时不虚构一次队列采样。
计时器使用 Tokio 单调时钟，测试只控制时钟与宿主 I/O 边界。
直接 drop run future 不是优雅停止，应 cancel 并 await。

split-scatter 每次真实计数变更同时发布 Counter；pending 在新增、删除、TTL回收和 clear
时更新 Gauge。重复回收不重复计数，clear 不清除累计 Counter。
接线时只发布已有 pending，不重放历史 Counter；必须在接受工作前接线。

[独立测试](task_058_test.rs)验证 schema、Registry 隔离、首次/末次注册碰撞回滚、未知事件、
真实 dispatcher/placement/affinity/MemoryStore/operator 巡逻、暂停、预取消、宿主错误、
backlog、容量拒绝和 attempted/unattempted TTL 分类。
[证据](../../../docs/rust/schedule-execution/task-058.md)记录确切命令、槽位、失败和通过结果。
