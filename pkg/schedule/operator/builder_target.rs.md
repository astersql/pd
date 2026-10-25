# Builder 目标校验

`Builder` 记录 target peers/leader、ExpectedRoles 与多目标 leader，首个错误锁定后
不再修改目标。`BuilderContext` 借用宿主 `StoresInfo`、同代配置和 RuleManager 的
apply-region 规则结果，不维护另一个集群或规则管理器。构造器使用真实 `fit_region`
冻结适用 RuleFit 的规则；`allow_leader` 使用真实异步 `StoreStateFilter` 和标签匹配。
时间由宿主显式传入，库不创建 runtime。`is_allowed_leader_target` 执行同样的 origin
准入后检查非强制候选。

Go 只读契约为 builder.go 的 NewBuilder/AddPeer/RemovePeer/PromoteLearner/
DemoteVoter/SetLeader/SetLeaders/SetPeers/SetExpectedRoles/prepareBuild/allowLeader；
相邻 TestNewBuilder、TestRecord、TestPrepareBuild、TestTargetUnhealthyPeer 提供断言
依据。本次依用户授权采用源码推导，未运行 Go builder 或声称跨语言运行差分。
完整固定输入、来源哈希与53项预期保存在 task_026.json。

保留 Go 边界：AddPeer 不在记录时检查 store 存在性；SetLeader 立即拒绝 learner
和 unhealthy peer，witness/store 状态在 prepare 目标校验阶段拒绝；force 可跳过
store 状态和 placement，但不能跳过 store 存在、learner/demoting/witness；原 leader
在角色与 witness 检查后跳过 store/placement。ExpectedRoles 不要求 key 已有 peer，
不存在或后来变为 learner 的目标 leader 在 validate 时清零。SetLeaders 只做存在、
learner 和健康过滤，排序后追加并保留重复项。两种 Skip 均只跳过对应 origin/规则检查。

Rust Peer 无 nil 表示，AddPeer 用 Option 暴露 nil 边界，origin 的 nil/store-zero
统一为 InvalidOriginPeer。SetPeers 用拥有值的 map 排除 nil entry，并检查 key/store
一致性、store-zero 和 joint。错误用稳定 enum 保存源码分支身份；不依赖 protobuf
文本打印，缺失/非法/不健康类别保留，未声称错误文本逐字 wire 对齐。

`validate_targets` 在规划前拒绝无 voter 或非法 leader，且失败保持 sticky。
`build` 本阶段仅实现无变化（包含仅改 peer ID）时的 NoSteps，其他有效变更返回明确
PlanningRequired 交给后续规划任务；不生成空步骤 Operator，不分配 ID，不进行 I/O。
普通规划现通过显式 `build_simple` 入口接入，复用目标校验并由宿主提供 ID 分配；
其范围和验证见 [普通规划](builder_simple.rs.md)。witness 转换和 joint 规划仍属后续行为族。
不存在存储读写，因此不添加 MetaStore 桩或持久化故障测试。

验证入口为独立注册 task_026_test.rs：53个 fixture 案例，以及多目标、所有权、
候选准入和 no-step 测试。四种行为变异分别证明重复添加、force/store、placement
和角色/witness 错误会被断言捕获。运行命令、有效数量和共享槽位见
[任务证据](../../../docs/rust/schedule-execution/task-026.md)。

任务28增加内部 `allow_leader_at(peer, force, current_leader)`，规划阶段豁免当前
leader，避免把已转出的原 leader 永久豁免。公开 `allow_leader` 和 prepare 校验继续
使用 origin leader，角色/witness 检查始终先于豁免。
