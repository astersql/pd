# 区域均衡调度

`BalanceRegionScheduler::schedule` 在宿主运行时借用真实 BasicCluster、配置、
RuleManager、统计快照、affinity 与 key-range manager。每轮最多返回一个真实
`PeerOperatorFactory::create_move_peer` 操作；无候选时返回空结果。
`is_schedule_allowed` 供协调器以当前 REGION 操作数和限额作严格小于判断。

`BalanceRegionConfig` 只保存范围，编码沿用 KeyRange 十六进制 JSON；空范围构造
归一为全 keyspace。Go 本调度器明确 TODO ReloadConfig，因此没有新增持久化更新路径。
HTTP、Prometheus counter 和控制器准入保留在宿主适配层。

来源是只读 `balance_region.go`、`utils.go` 和 `balance_region_test.go`。
按用户授权，预期来自源码与测试契约推导；本任务没有 Go 运行输出或运行差分。
来源哈希、固定 fixture 与实际 Rust 命令见
[任务证据](../../../docs/rust/schedule-execution/task-072.md)。

先筛来源与不可接收目标，再按带 pending influence 的 region score 降序排列来源。
区域选择依次尝试 pending、follower、leader、learner，使用真实 down、replicated、
snapshot sender、affinity、empty、witness、pending 过滤器。复制 influence 作为
本轮快照，避免共享查询接口补零污染调用方状态。选区后的热度、leader 检查与
placement safeguard 约束目标。目标由低分向高分尝试，已有 peer 不可重入。

复用 BalanceSolver 对来源与目标对称应用 `max(tolerance, candidate size)`，并把
pending influence 放大 5 倍后保守计分。只在投影来源分数严格大于目标时创建操作，
保留原 peer role、评分附加信息、store health check 和默认 MEDIUM 优先级。
重试配额沿用 RetryQuota；成功重置，失败衰减，轮末清理已消失 store。

随机选区、shuffle、时钟与 peer ID 分配是显式宿主边界。分配/构建失败保留
BuilderError，并在 dry-run 记录 CREATE_OPERATOR_FAILED；不存在内部 Tokio runtime。
`supports_conf_change_v2` 从宿主版本能力传入。范围视图宿主用
`allow_empty_regions` 表达 Go rangeCluster 的 empty-filter 豁免；范围投影仍使用
已有 RangeCluster。其独立 tolerance override 由宿主配置快照提供，本模块不复制
range 状态或推断宿主具体类型。

测试覆盖 22 个固定调度场景、5 个评分边界、范围 JSON 编码/坏 hex/空范围和限额。
成功路径逐条执行 AddLearner、PromoteLearner、必要的 TransferLeader、RemovePeer，
核对实际成员索引、REGION kind、MEDIUM、影响量、confver=3 与分布改善。
单个非空区域在交换 store ID 后各重复调度 10 次仍不搬迁；故障在 allocator I/O
边界注入。故意绕过评分准入时，large-overshoot 断言失败，恢复后通过。

服务端任务66的真实Go/Rust诊断差分发现，目标排除提前continue会漏报
`StoreAlreadyHasPeer`。目标现在复用`ExcludedFilter`，dry-run把拒绝原因记入
同一BalanceSchedulerPlan；正常调度的目标准入保持一致。当前运行差分和
验证见[任务66](../../../docs/rust/server-api/task-066.md)。
