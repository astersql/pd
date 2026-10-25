# 增加副本与晋升步骤

`AddPeer`、`AddLearner`、`PromoteLearner` 是 canonical operator crate 的领域步骤。
它们使用真实 `RegionInfo`、`BasicCluster`、`SharedConfigProvider`、`OpInfluence`
及 hbstream `Operation`；不依赖 protobuf、存储或私有运行时。

`is_finish` 只检查最近一次区域快照。增加 voter/learner 需要 ID、witness 属性
及对应角色的 pending 检查通过；晋升只接受普通 Voter，忽略 pending/witness。
Go 中联合角色属于非 learner，增加 voter 可完成，晋升不会完成。
`conf_ver_changed` 不等待 pending；learner 增加只检查 store peer ID。
Go nil-safe getter 的 ID=0/默认 Voter 语义保留，因此零 ID 在缺失 peer 时的
confver 可能为1，但 `is_finish` 仍为 false。

`check_in_progress` 对增加步骤先检查 store 存在、可选 unhealthy、严格大于
配置 down time，再检查 ID 冲突。增加 learner 还拒绝已有非 learner。
晋升只检查 ID，不检查 store 健康或角色。时钟以显式 Unix 纳秒传入。
错误 `PeerStepError` 保存类型和冲突 ID，字符串与来源一致。

`get_cmd` 在增加目标上已有任何 peer 时返回 None；晋升总返回 AddNode。
v1 为单条 ChangePeer，v2 为单元素 ChangePeerV2；保留 witness 属性。
本任务不实现 witness 转换或 joint 操作。

影响量增加目标 region count；witness 增加 witness count，其余增加 region size。
轻量或 witness 不收费。learner 可额外对 SendStore 收取原始 region size 的
snapshot 成本（即使空区域；SendStore=目标也正常累加），SendStore=0 不收费。
所有整型累加/慢速超时按 Go 的有符号 wrapping 处理；慢速等待至少10分钟，
晋升复用前置 TransferLeader 的快速等待计算。晋升不改变影响量。

来源只读 `step.go`、`step_test.go`、core region 和 storelimit 契约；测试期望
依据源码固定，用户授权不要求 Go 运行 oracle。未运行 Go 调度测试，未宣称运行差分。
独立 `task_019_test.rs` 验证真实状态、错误和完整命令；
[证据](../../../docs/rust/schedule-execution/task-019.md) 记录红绿测试、案例和基线。
