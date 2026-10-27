# Witness 均衡领域边界

`BalanceWitnessScheduler::schedule` 从 canonical BasicCluster 的 witness 索引采样，
只在已存在的非 witness voter 之间交换角色。复用 StoreState/SpecialUse、
PlacementGuardContext、BalanceSolver 和 RangeOperatorFactory；没有传输或 protobuf。
返回真实 Operator、诊断计划与类型化 BuilderError。宿主先调用
`is_schedule_allowed` 比较在途 WITNESS 操作数与限额，再请求调度。

`BalanceWitnessConfig::new` 验证 batch 1–10，默认4，范围沿用二进制 key 的
十六进制 JSON 编解码。持久化见 `balance_witness_config.rs`；HTTP见
`pkg/server/schedule_admin_balance_witness.rs`（任务139）。
`from_transport_parts` 保留带符号batch和可空ranges，供HTTP兼容Go部分解码状态，
不校验、不保存。`transport_batch`/`transport_ranges`读出精确状态；`batch()`保留
原usize接口，负数错误状态读为0；正常构造与领域更新仍验证1–10。
JSON恢复保留null/[]，采样将空ranges解释为全域，不改变调度算法。
时钟与随机索引由宿主提供，规则管理器和状态均借用，库不创建 runtime。
角色交换保留 peer ID，无需分配新 ID，工厂若意外请求分配将返回 Allocation 错误。

只读 Go `balance_witness.go` 的 Schedule 只选择最高分来源，重试其 witness 区域，
一次最多返回一个操作（即使 batch>1）。不能把配置名称推断为多操作循环。
区域先过滤 pending/down；目标是现有非 witness voter，按 witness 分数升序；
启用 placement 时阻止 leader 目标并检查 RegionFit.Replace。
Replace 按 Go 顺序先允许同规则/同 role 的交换，然后才检查标签和位置。
评分复用按数量计算的 tolerance 与保守在途影响，不改变调用方 influence。
不额外添加 Go 此路径没有的 hot、affinity 或全复制过滤。

测试应用实际联合/普通角色切换步骤到 RegionInfo，检查成功终态、四次 confver、
成员和 leader 保持、双向 witness influence 及 canonical 索引计数。
无 leader 由工厂返回 NoLeader，作为 step4 创建失败传播，不静默丢弃。

来源为 Go 源码及相邻测试契约推导，未执行 Go，也没有声称运行差分。
固定场景、失败/通过日志和验证边界见
[任务073证据](../../../docs/rust/schedule-execution/task-073.md)。
