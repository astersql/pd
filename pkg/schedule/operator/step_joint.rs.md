# 联合共识进入与退出

`ChangePeerV2Enter`、`ChangePeerV2Leave` 在 canonical operator crate 中公开，
复用 core RegionInfo/Peer、PromoteLearner、DemoteVoter 和 hbstream Operation。
只处理成员角色；不触发网络、持久化或运行时生命周期。无新增依赖。

来源为只读 `step.go` 的同名步骤及 `step_test.go` 的 TestChangePeerV2Enter、
TestChangePeerV2EnterWithSingleChange、TestChangePeerV2Leave。依用户授权使用
源码推导期望，未运行 Go，未声称 Go/Rust 运行差分。冻结来源哈希、输入与期望见
[fixture](../../../docs/rust/schedule-execution/fixtures/task_022.json)。

进入的 finish 接受 incoming/voter 与 demoting/learner；confver 还接受删除的
降级目标及符合原始 ID 的 witness 转 data。退出复用单成员稳定角色检查，且要求
全区域没有 joint peer；confver 不附加全区域 joint 检查。pending 不阻塞这些步骤。
进度检查按 promotion、demotion、原子性、额外 joint peer、demoting leader 顺序
返回 `JointStepError`，保留原始错误文本和 Go protobuf nil getter 的零值行为。

两个步骤无影响量。timeout 使用 fast timeout × (成员数+1)，`timeout_nanos`
保留 Go 有符号 nanosecond 溢出；`Duration` 视图保留其二进制位，调用方需要有符号
deadline 时应使用前者。V1 返回 None，进入 V2 保留 promotion→demotion 顺序与
重复成员，退出 V2 返回显式存在的空 changes envelope。Display 按原 Go 文本，
不将 witness 字样加入联合步骤描述。

[测试](task_022_test.rs) 覆盖固定角色表、缺失/替换、部分切换、额外 joint peer、
leader 安全及错误优先级、witness、pending、空/单成员、命令、影响量及超时。
[验收证据](../../../docs/rust/schedule-execution/task-022.md) 记录红绿验证、
完整 workspace 基线与只读核对。
