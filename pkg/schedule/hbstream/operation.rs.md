# 心跳领域命令

`Operation` 保留 Go `hbstream.Operation` 的七个独立可选动作；不是互斥枚举。
默认值表示全部缺省，存在但为空的 ChangePeerV2 等动作仍保留，false 的
ChangeSplit 仍是有效动作。ChangeType、SplitPolicy 保留未知 i32 数值。

Peer、RegionEpoch、Merge 的 RegionMeta 复用 canonical core，包含 role/witness、
二进制边界、全部 epoch、encryption 和 flashback 字段。Merge 使用 core 的正常
region membership（每个成员是 Peer）；不引入另一份 region 模型。
TransferLeader 候选、ChangePeerV2 批次与 witness 批次保留顺序和缺省条目。
Split keys 保留二进制内容、空 key、顺序及重复项。

`HeartbeatCommand::for_region(cluster_id, region, operation)` 从真实 RegionInfo
快照目标 leader、ID 和可选 epoch，并接收拥有所有权的 Operation。无 leader 返回
None；空动作和零 peer 不增加 Go 没有的校验。命令可以独立排队和 clone，不借用
输入。该层没有协议依赖、网络发送、后台任务、runtime 或 I/O 错误；流生命周期和
wire 转换由后续任务实现。无 leader 是本任务的故障/抑制边界。

[task-012 证据](../../../docs/rust/schedule-execution/task-012.md) 记录真实 Go SendMsg
在 classic 与 scheduling 下的 34 组冻结结果、TDD 和 workspace 验证。
JSON 仅在测试中读取 oracle；生产 API 不依赖 serde 或协议类型。Go JSON 省略的
默认标量恢复为领域默认值，nil/empty byte slice 均表示空字节序列；消息缺省和
空消息、缺省列表项、枚举数值及业务字段均不丢弃。Go Marshal 不支持的失败不被
伪造为本领域的错误。
