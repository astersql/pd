# 范围与 witness 操作工厂

`create_split_region_operator` 保留输入 kind、策略（含未知枚举值）、有序二进制
keys、区域 epoch/范围，拒绝 origin joint；附加信息使用大写十六进制 key。
不擅自校验或排序 split keys，与 Go 的直接传递契约一致。

`RangeOperatorFactory` 借用已有 `BuilderContext`，持有区域快照；宿主传入
ConfChangeV2 能力、peer ID 分配器和 scatter 随机索引。无私有 runtime、网络、
协议模型或持久化。`is_region_match` 比较 store、learner 身份、witness 身份和数量，
不比较 peer ID、leader 或顺序。

- `create_merge` 在 membership 不一致时通过 canonical Builder 对齐 source，
  保留旧 ID、为新 peer 分配 ID；返回 active source/passive target 两个操作，
  同步 timeout 和双向 related region 信息。active 等待 source 消失，由后续控制器
  负责完成；passive 观察目标范围改变。任一端 joint 时拒绝。
- `create_scatter` 接收完整目标 peers、显式 leader、随机索引、skip-limit、admin
  与 allocator。零 leader 时，从按 store 排序的非 learner 候选中选取宿主提供的
  索引（模候选数）；这替代 Go map 迭代加 rand.IntN 的随机边界，不承诺相同种子
  得到相同 leader。admin 强制目标，non-admin 遵守 leader scheduling 限制；
  add 总是轻量，remove 仅在 skip-limit 时轻量，peer/health 校验仍有效。
- `create_witness`、`create_move_witness` 仅组合 Builder 意图；身份、leader 安全、
  降级/快照/恢复 voter 和批量步骤由已验证规划器负责。
- `create_leave_joint` 重建 Builder，显式跳过 origin joint 和 placement fitting。
  内部 `Builder::build_leave_joint` 在 builder_joint.rs 复用现有 Planner 的 leader
  偏好和负载评分：先普通目标，再强制目标，最后仍无目标时只发 leave。不会重新
  enter、不会把 incoming 当 learner，也不将原 demoting leader 留作转移目标。

错误沿用 `BuilderError`，新增 `NotJoint` 表示无需恢复；分配失败不返回半成品。
成功输出使用现有 Operator/OpStep，不新增状态面、Cargo 依赖或 features。

来源是只读 `create_operator.go` 与 `builder.go`，相邻测试包括
`TestCreateSplitRegionOperator`、`TestCreateMergeRegionOperator`、
`TestCreateLeaveJointStateOperator` 和
`TestCreateLeaveJointStateOperatorWithoutFitRules`。按用户后续授权，fixture 明确
标为 `source_assertion`、`go_execution=false`；没有 Go 运行差分声明。
测试覆盖完整步骤/身份/kind、错误、分配故障、输入所有权和真实 RegionInfo 心跳
回放；证据见 [task-032.md](../../../docs/rust/schedule-execution/task-032.md)。
