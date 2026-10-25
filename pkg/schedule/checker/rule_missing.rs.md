# rule_missing.rs

领域入口为 `RuleMissingChecker::check`、`check_with_fit` 和
`fix_missing_tiflash_learner_peer`。仅负责 placement 缺失副本阶段，使用真实
RuleManager/RegionFit、ReplicaStrategy、LabelConstraintFilter 和 PeerOperatorFactory。
不复制 placement 状态，不依赖 protobuf/server/runtime，也不安装运行时。

宿主传入 coherent config/store 快照和版本及配置计算后的 `witness_enabled`
（可复用 checker 的 `is_witness_enabled`）；Go 的 Leader/Follower/Voter 都创建
voter peer，Learner 只添加 learner。witness 按 capability gate 生效。
补副本操作 `add-rule-peer` 使用 REPLICA|REGION、HIGH，ID 分配错误以
`RuleMissingError::Builder` 返回，region/epoch 和有序步骤由 canonical builder 保留。

`check` 经 canonical manager 拟合；`check_with_fit(None)` 表示规则关闭。
暂停、无 leader 时不失效缓存；有 fit 则先失效，逐条补缺失规则，第一条成功即返回，
保留前面规则错误。无操作且满足 placement 的有效 region/store 可以成为缓存候选。
缓存本身由 RuleManager 管理，epoch/leader/rule/store 变更由既有实现判定。

无目标时，普通补副本按临时/永久状态返回 pending_processed/pending 意图；
临时状态请求移除 pendingList 并插入 pendingProcessedRegions，永久状态插入
pendingList；返回 operator 请求移除 pendingList。暂停等无动作结果不触碰宿主
已有 retry cache。目标缺失时可先搬走占用满足约束 store 的其它已满足规则 peer
（swap-fit）；typed NoStoreToReplace 与 builder 错误保留，witness swap 使用 URGENT。
这里只实现缺失修复需要的 swap 规划；offline recorder、异常角色和 orphan/位置改善
由后续任务组合，不将本阶段当作完整 RuleChecker controller。

TiFlash offline 预检只接受 learner + `engine in [tiflash]` 的缺失规则，逐一
跳过无目标规则；不触碰 retry cache，不尝试 swap-fit。普通缺失检查仍可对其它
learner engine 规则执行补齐。预检的调用前提由后续 offline 阶段负责。

来源为只读 `rule_checker.go` 的 Check/CheckWithFit、addRulePeerWithOptions、
fixMissingTiFlashLearnerPeer/isTiFlashLearnerRule、strategy/handleFilterState，
以及相邻 Go 测试的契约。按用户授权使用源码推导，未运行 Go，不声称运行差分。
完整 provenance、失败/通过及错误行为识别见
[任务043证据](../../../docs/rust/schedule-execution/task-043.md)。
