# 操作结果语义比较

`check_operator(Option<&Operator>, &OperatorExpectation)` 只读比较真实调度操作。
`OperatorExpectation` 对应 Go 的十类公开 Check 工具，`OperatorMismatch` 区分
缺少操作/期望步骤、步骤数量/字段、kind 和 leader 目标错误。错误实现
Display/Error，不依赖测试框架、不 panic、不推进 Operator 状态。
该 crate 供调用方以 dev-dependency 使用；生产调度 crate 未增加反向依赖。

依据只读 `operator_check.go`：kind 按必需位包含关系比较，允许额外位；
single leader 比较完整 TransferLeader，multi-target 保留有序 ToStores 精确比较；
from/multi-source 只比较来源。peer/learner 忽略所有 leader 步骤，peer 支持
AddLearner/PromoteLearner/RemovePeer 和四步 joint 序列。带 leader 的 peer
使用最后一个 leader 目标，必须非零且离开 source，target=0 是通配。
learner 不接受 joint，AddPeer 不核对 PromoteLearner 的 store；RemovePeer
不核对 kind。MergeSteps 要求 MERGE 位，按步骤种类投影比较目标、来源或
passive，不比较 peer ID、候选列表和 region 元数据；空但存在的期望列表合法。
Go 类型断言 panic 在这里转换为 Step 错误；缺少操作总是 MissingOperator。

Go oracle 为源码/已有测试推导，经用户授权替代缺失运行输出；没有新增 Go
运行或运行差分。冻结输入、期望及不归一化策略见
[fixture](../../../docs/rust/schedule-execution/fixtures/task_133.json)，验证见
[task-133](../../../docs/rust/schedule-execution/task-133.md)。
