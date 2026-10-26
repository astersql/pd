# 调度诊断结果聚合

`DiagnosticRecorder::new` 仅接受 balance-leader / balance-region 类型，其他类型返回
`None`（对应 Go 的 nil recorder）。宿主负责 Option 分支及共享访问同步；配置在
`is_allowed` 调用时读取真实 SchedulerConfigProvider。时间由宿主传入 Unix 秒，
领域库不读取系统时钟、不建 runtime、不提交操作器、不写存储。

`analyze` 复用 canonical `balance_plan_summary`：有真实操作器时优先返回 scheduling，
否则汇总真实 BalanceSchedulerPlan 的最深阶段、状态优先级与频次，返回 normal 或
pending。空计划是 normal。保持 Go 忽略 summary 错误的兼容行为：错误计划类型为
pending/空 store map；必要资源缺失沿用计划层 panic，入队在分析完成后进行，故障
不会污染历史。`set_result_from_status` 保留任意状态字符串。

历史最多十条，重复时间戳不合并，只聚合最新连续同状态段。pending/normal 对每个
store 的状态使用三段 3/2/1 权重，余数分到较新段；使用整数单位表示相对权重。
相比 Go 浮点归一化，这避免了数学并列处的舍入偏差。最大权重并列取较小的
(code, detailed_reason)，汇总组按该键排序：这是对 Go map 无指定顺序的确定化，
不宣称并列状态与任意一次 Go 执行完全相同。详细 reason 参与分组，但文字与 Go
Status.String 一样只输出状态名。无 store 的 pending 输出 `<type> reach limit`。
其他状态的 summary 为空。输出时间取查询时刻，内部 StoreStatus 不序列化，且查询
返回独立值，不能修改历史。

来源为只读 diagnostic_recorder.go、scheduler_controller.go、balance_plan.go、
weight_allocator.go、fifo.go；按用户授权使用源码推导，没有新 Go 运行 oracle。
真实 dry-run 测试调用既有 BalanceLeaderScheduler，使用真实 core、RuleManager、
MemoryStore、labeler、operator 与 influence：成功产生 leader transfer；移除节点
产生 StoreRemoved 计划并聚合；输入 region leader、store 计数和 influence 保持不变。
RuleManager 的既有缓存与 scheduler 内部重试配额不属于全局调度状态写入。

验证、失败与变异日志见 [任务证据](../../../docs/rust/schedule-execution/task-109.md)。
