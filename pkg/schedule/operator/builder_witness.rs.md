# 见证角色步骤规划

`Builder::become_witness` / `become_non_witness` 记录目标，保留 role、peer ID 和
输入所有权。缺失 peer 返回 MissingPeer，重复转换返回 AlreadyWitness /
AlreadyNonWitness；沿用 Builder 首错语义。`WitnessPlan` 是两条 canonical
规划路径内部共享的差异对象，不建立另一份集群状态或协议模型。

来源为只读 `builder.go` 的 BecomeWitness、BecomeNonWitness、prepareBuild、
buildStepsWithoutJointConsensus、buildStepsWithJointConsensus、execBatchSwitchWitnesses。
固定期望来自源码推导，`go_execution=false`，不宣称 Go 运行差分。
builder_test.go 的 TestBuild 是成员/leader 顺序的相邻契约，没有 witness 专项场景。

普通路径在成员变更后依次生成 BecomeNonWitness、恢复 PromoteLearner、BecomeWitness。
目标为普通 voter 的 witness 先以 witness learner 参与成员规划；joint 路径降级，
普通路径 remove/add；快照完成后才恢复 voter。联合路径将切换按 store ID 排序
合并为 BatchSwitchWitness，随后发恢复 voter 的 enter，多项恢复还需 leave。
单项简单切换仍走普通路径。新增 witness 沿用真实 AddLearner/PromoteLearner。
SendStore 保持 origin leader；现存 peer ID 取 origin；分配失败传播 Allocation。

leader 选择排除待恢复 voter；显式指定这类 peer 返回 LeaderNotAllowed。
联合路径先转移即将变 witness 的 leader，再切换。保留来源 OpKind 差异：普通
转换增加 REGION，联合批次切换本身不增加 REGION，恢复 voter 时增加 REGION。
不执行持久化或网络 I/O，不创建 runtime；控制点只有宿主 ID allocator。

验证见 [task-030](../../../docs/rust/schedule-execution/task-030.md)。独立测试回放
真实 RegionInfo，核对完整步骤、身份、最终角色、心跳完成/confver、switch 命令
和快照 pending 阻止完成；未启动 TiKV，未声称分布式 Raft 验证。
