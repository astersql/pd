# 联合共识构建器

`Builder::build_joint(kind, SimpleBuildOptions, allocate)` 是显式启用 joint
consensus 的领域入口，复用 canonical Builder、Planner、leader 评分及 OpStep。
宿主提供 stores/config 快照与 ID 分配函数，不创建 runtime、不接触 wire 或持久化。
`Builder::build` 的既有目标校验入口仍由后续创建器装配；本 API 与
`build_simple` 并列。接线作为 builder_simple 的局部子模块，以复用其私有 Planner。

来源为只读 `builder.go` 的 prepareBuild、buildStepsWithJointConsensus、
execChangePeerV2，以及 `builder_test.go::TestBuild`。固定 fixture 的 provenance
是 `source_assertion`、`go_execution=false`，依据用户授权使用源码契约推导，
未取得本故事 Go 运行输出，也不宣称运行差分。

新增副本先以 learner 添加，移除 voter 先降级；按 store ID 排序形成 promote/demote
列表。已有目标 voter 在 enter 前接 leader，保留 origin leader 时在 leave 后切换，
同时降级原 leader/晋升目标时在 enter 与 leave 间切换。多成员变更必须显式 leave；
单成员 V2 变更由 TiKV 自动稳定，不发 leave。单项非降级变更复用普通路径，
但移除后仅剩一个 peer 时保留 V2 路径。仅 promotion/demotion 不添加 REGION kind，
与 Go 的 TestBuild 断言一致。保留输入 kind、原 peer ID、witness 标志及轻量标志。

默认拒绝已有 joint 状态，沿用 BuilderOptions 的显式 origin-check 豁免语义，
不将豁免解释为修复已有 joint 的能力。witness 转换由 [witness 差异规划](builder_witness.rs.md) 接入：成员变更后批量
切换，快照完成后单独恢复 voter；未变更的 witness 副本可参与成员规划。
所有构建错误写回 Builder 首错状态；allocator 失败不会返回部分 Operator。

`task_029_test.rs::task_029_behavior_cases` 用固定21场景断言完整步骤、目标、kind、
身份、错误及输入所有权；在真实 RegionInfo 上逐步骤回放心跳角色变化，验证
is_finish/conf_ver_changed、leader 资格和最终非 joint 状态。单成员自动稳定是
显式模拟的外部 TiKV 心跳行为，未启动 TiKV 或声称分布式 Raft 验证。
失败、通过和 workspace 证据见 docs/rust/schedule-execution/task-029.md。
