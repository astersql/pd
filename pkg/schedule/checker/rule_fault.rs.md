# rule_fault.rs

`RuleFaultChecker::check_with_fit` 消费 canonical `RegionFit`、`RegionInfo`、
`StoresInfo` 与 `CheckerConfigProvider`。按 rule 顺序复用任务43的补缺实现，
修复 down/offline，再处理 loose match；orphan 清理与位置优化由独立阶段负责。
宿主持有 checker 并串行调用，不创建 runtime、定时器或全局缓存。

故障替换通过真实 `ReplicaStrategy` 施加标签、isolation 与 store budget 约束，
再调用 `PeerOperatorFactory`；TiKV down 的 fast failover 使用 witness 和 URGENT，
TiFlash 保持数据副本。offline 首先尝试真实 TiFlash 缺失 learner 添加。
leader 替换按 offline 计数、peer 健康和 `allow_leader` 选择目标，只有构建成功
才记数；五分钟后的 refresh 按 store ID 查找缺失/removed store 并清空计数。

角色修复覆盖 learner 晋升、leader/follower 转移、voter 降级及 witness 转换。
`record_region_promote_to_non_witness` 是实际完成后的宿主回调，显式传入当前时间
和配置间隔；pending peer 和未到期记录都会抑制切换回 witness。已有记录保留原
到期时间，配置更新只影响新记录。`allow_leader` 使用真实 StoreStateFilter（包括
leader budget）及 leader/voter 标签规则，拒绝 learner、witness 和缺失 store。

`RuleFaultError` 保留 NoStoreToReplace、角色拒绝和真实 BuilderError；
`RuleFaultOutcome` 携带 operator、pending/pending_processed 意图和按顺序保留的
各 rule 错误。offline TiFlash 预检失败遵循 Go 的降级替换，不向成功路径附加错误。

Go 契约来自 `rule_checker.go` 的 fixRulePeer、replaceUnexpectedRulePeer、
fixLooseMatchPeer、allowLeader、recorder.refresh；不是 Go 运行差分。
用户已解除运行 oracle 准入限制。独立注册的 `task_044_test.rs` 使用冻结 fixture、
真实 placement/filter/builder 与实际心跳推进验证，无 mock 调用计数。
详见[任务证据](../../../docs/rust/schedule-execution/task-044.md)。

任务113将 witness repair 的现有到期判断提取为 `is_recently_promoted`；
Coordinator 的 heartbeat 完成记录与修复路径查询同一个 map，严格 `until > now`。
