# 删除与降级副本步骤

`step_remove.rs` 使用已有 core `RegionInfo`、`OpInfluence`、storelimit 及
hbstream 领域命令，没有 protobuf、存储 I/O 或运行时依赖。

`RemovePeer` 的 ID 仅决定 confver 是否已变化。替换后的 peer 不代表整个删除步骤
完成；完成要求 store 上没有 peer。进度检查只拒绝当前 leader，错误为
`RemovePeerError::LeaderPeer`，不要求 store 存在或健康。nil leader 的 store ID
按 Go getter 视为 0。命令始终发送 heartbeat 当前的 peer，包括缺失 None 和
替换后的 ID，不使用步骤 ID 合成删除对象。

影响量使用 peer-local size/keys：witness 通常无数据，但仍减少 region/witness
计数；轻量步骤不收费；down store 将计费大小上限设为 SmallRegionThreshold。
继续复用空区域判定与标准/小区域成本表，有符号累加按 Go wrapping 语义。

`DemoteVoter` 是 joint-consensus 子步骤，Go 没有独立 CheckInProgress/Influence，
因此不虚构这两个 API，也不禁止 leader。完成要求对应 ID 的 Learner；
DemotingVoter 尚未完成，pending learner 可完成。confver 还接受缺 peer 或预期
witness 已变成非 witness；后一个条件不要求 ID 相同。命令携带步骤 ID、Learner
角色及 witness 标志，支持 v1/v2 包装。两类步骤复用既有 fast timeout 契约。

来源为只读 `step.go` 的 RemovePeer/DemoteVoter 方法、`region.go` 的 peer-local
数据 getter、`influence.go` 与 storelimit 成本表。相邻 `step_test.go` 的
ChangePeerV2Enter/Leave 案例证明联合角色背景，未声称存在独立 RemovePeer 或
DemoteVoter 测试。用户授权使用源码契约；没有运行 Go oracle，没有运行差分声明。

测试、冻结 fixture、红灯 mutation 与全 workspace 证据见
[task-020](../../../docs/rust/schedule-execution/task-020.md)。
