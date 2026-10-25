# 规则孤儿与位置改进

`RuleLocationChecker::fix_orphan_peers` 和 `fix_better_location` 消费真实
`RuleMissingContext`、`RegionInfo`、`RegionFit`/`RuleFit`，返回 canonical
`Operator`、`BuilderError` 或已有的 `RuleMissingOutcome`。宿主按 Go 的顺序在
逐条规则修复之前调用孤儿清理，在每条规则缺失/异常/角色修复之后调用位置改进。
此模块不复制批次44的故障修复或 recorder，也不建立 controller、缓存、运行时或存储后端。

孤儿删除首先保护规则计数、角色和 fitted peer 健康；pending 阻止 pin，down 或
断连（包括缺失 store）可确定 pin。健康 orphan 仅在替换不降低 fit、多样性且双方
均非 witness 时代替断连 pin，角色转换复用真实工厂。健康 fit 直接清理首个 orphan；
不健康 fit 下至少两个 orphan 才允许清理，优先 unhealthy，再按 extra_count 和
断连优先级保留备用。精确保留 Go 的遍历顺序，不将“断连”与 pending/down 合并。

位置改进只处理配置 location labels 的规则。复用 `ReplicaStrategy::get_better_location`
进行同 engine、匹配规则角色的多样性比较，复用 label/isolation/state/storage/special-use
过滤。witness 使用 fast failover，最终仍为 MEDIUM 的 REPLICA 操作。无目标时返回
`pending = !temporary`、`pending_processed = temporary`；工厂错误保留类型，无半成品。
暂停及无 leader 安全返回；调用者输入不变。

契约来自只读 `rule_checker.go` 的 fixOrphanPeers、fixBetterLocation、handleFilterState
及 Go 相邻测试，**仅源码推导，未执行 Go，也不声称运行差分**。固定输出、开发失败、
错误行为识别、真实心跳推进和回归见[任务45证据](../../../docs/rust/schedule-execution/task-045.md)
及独立注册的 [task_045_test.rs](task_045_test.rs)。领域阶段消费 snapshot，没有新增
MetaStore 写入；故障验证在 allocator I/O 边界注入错误。配置/version 能力由宿主提供，
joint 降级使用 canonical ChangePeerV2Enter；不新增 feature 或依赖。
