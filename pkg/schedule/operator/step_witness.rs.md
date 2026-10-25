# 见证副本切换步骤

`step_witness.rs` 提供 `BecomeWitness`、`BecomeNonWitness` 和
`BatchSwitchWitness`，使用 canonical core、config、OpInfluence 与 hbstream
领域类型，无 protobuf、运行时、持久化或外部 I/O。

完成与 confver 不检查 peer 的投票角色，只检查 store/ID/witness 标志。
转非见证的 confver 不等待 pending，完成则查询所有角色的 pending peer。
缺 peer 永不完成；Go nil-safe getter 使 ID=0 的缺失非见证 peer confver 为1。
批次要求全部子步骤才确认 confver，并返回总数；空批次完成而 confver 为0。

复用 step_add 的包内 store 校验与 slow timeout，无额外公开依赖。错误复用
`PeerStepError`：先验证 store，再检查 peer 是否存在且匹配，按顺序返回首错。
转见证忽略额外 unhealthy 检查但仍拒绝 down；转非见证透传 health 参数。
批次总是先检查转见证、再检查转非见证。

影响量使用 region 全局 size/keys，增加/减少 witness 数并扣减/增加 region size，
不改变 region count；分别收 RemovePeer/AddPeer 成本。非零 SendStore 单独收
SendSnapshot 原始 size，可与目标相同且不会嵌套持锁。影响量采用 Go wrapping
语义，不依据 heartbeat 的当前 witness 状态抑制重复调用。

命令忽略 v1/v2 和 heartbeat 当前状态，固定发送步骤 peer ID，先转见证再转非见证，
保留重复及空批次的 present envelope。Display 遵循同一顺序。
超时分别使用 fast/slow，批次为 slow×(非见证数+1)。`timeout_nanos` 保存 Go
有符号 time.Duration 的精确溢出结果；`timeout` 是无符号 Duration 位模式视图，
调用方处理可能溢出的有符号 deadline 时必须使用 `timeout_nanos`。

只读来源 `step.go` 的三类步骤与 helpers，`step_test.go` 的 TestSwitchToWitness
和 TestNeedStoreHealthCheck；无现成独立批次/非见证测试。按用户授权采用源码
推导期望，未运行 Go oracle，不声称运行差分。固定来源、红绿测试和验证见
[task-021](../../../docs/rust/schedule-execution/task-021.md)。
