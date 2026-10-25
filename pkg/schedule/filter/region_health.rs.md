# 区域健康与区域过滤

`region_health.rs` 在 canonical filter crate 中实现 Go `healthy.go` 与
`region_filters.go` 的行为。`is_region_healthy` 同时拒绝 down/pending；
`is_region_healthy_allow_pending` 仅拒绝 down。健康判断不检查 learner 或副本数。

`RegionReplicationContext` 借用宿主配置、store 快照与真实 `RuleManager`；显式
纳秒时钟用于 placement cache。`is_region_replicated` 与
`RegionReplicatedFilter` 在规则模式执行真实 fit，legacy 模式要求副本数完全
相等且没有 learner。过滤器只缓存成功 fit，失败保留上一次成功的 Arc。
RefCell 只提供本次同步调度求值所需的内部借用，不复制 manager 状态。

`RegionFilter` 接收真实 RegionInfo 的 Arc 并返回完整 plan Status。
pending/down 返回 1001；规则/witness 返回 1004；legacy 副本数返回 1003。
空区域只比较 approximate_size > 1 或总区域数 < 100，不检查 key 数。
`SnapshotSenderFilter::new` 在调用方 runtime 异步查询真实 store limiter，
按指定优先级捕获可发送且不 busy 的 store 集合；不预留 token。选择时仅检查
leader store（无 leader 使用 0），返回 1008。此集合不会随 limiter 后续变化。

`AffinityFilter` 借用真实 affinity Manager 和 RegionLabeler，以宿主秒时钟查询
regular_scheduling_allowed；无 manager 或无匹配 group 时放行，禁止时返回
1009。无新 feature、私有 runtime、协议领域类型或持久化写路径。

`select_regions` 保留输入顺序、重复项与 Arc 身份；`select_one_region` 短路返回
第一个通过者，并只将每个失败 region 的首个拒绝写入 collector。collector 的
base step 由调用方选择；region 阶段为 1，沿用 plan 的资源类型校验。

[任务证据](../../../docs/rust/schedule-execution/task-010.md)记录冻结 Go oracle、
45 组输入、完整状态与布尔返回、缓存行为、TDD 变异和 workspace 检查。
[独立测试](task_010_test.rs)使用真实 MemoryStore-backed RuleManager/labeler、
真实 limiter/affinity/core 对象。过滤器本身不执行持久化 I/O，故不虚构无关
存储故障；错误/故障覆盖 down、pending、欠/过副本、learner、规则拒绝、
witness、快照耗尽/busy、无 leader 和缺失 affinity manager。
