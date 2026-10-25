# 普通副本步骤规划

生产入口 `Builder::build_simple(kind, SimpleBuildOptions, allocate)` 显式选择非 joint
路径，复用 task026 的目标校验、真实 `StoresInfo`、配置、placement 和 leader 过滤。
原 `build` 入口保留前序阶段的 NoSteps/PlanningRequired 契约，后续 joint 路由任务
可统一接线；这里不根据尚未实现的 joint feature 猜测路径。

Builder 冻结 description 与 RegionInfo，生成真实 Operator 的 region ID、epoch、size
和 OpStep。新副本先 AddLearner，再按目标角色 PromoteLearner；降级使用 RemovePeer
后 AddLearner。旧 store 的 promote 保留原 peer ID，新增/重加的零 ID 通过宿主闭包
分配，错误以 `BuilderError::Allocation` 传播；构建失败不返回半成品 Operator，首个
构建错误保持 sticky。规划不执行外部 I/O、不创建 runtime、不安装全局状态。

替换优先于独立 promote/remove/add；替换枚举现存 leader、晋升后 leader、新增 voter，
按位置标签前缀（复用 core 的 Unicode equal_fold）、up store、旧 peer、提前加入目标 leader、目标 leader、减少迁移次数
进行字典序评分，相同评分保留 store ID 顺序较早的方案。最终 leader 排除 ExpectedRoles
中的 Follower，按角色/up/current/keep voter/old 和 count/size leader score 选择。添加的 SendStore
始终使用 origin leader；移除携带 down-store 与 light 标记；add/remove 和普通 witness 转换增加
REGION kind，独立 promote 保留来源不自动增加 REGION 的行为。leader 变化增加 LEADER。
多候选 leader 使用 SetLeaders 排序并过滤后保存的顺序，构建时不再次过滤，透传到 TransferLeader。

`NoValidLeader` 表示无法生成下一项合法计划；`NoSteps` 表示无变化（仅 ID 变化也无步）。
role/witness 的 target 校验复用前序 API；已有 peer 的 witness 转换由 [witness 差异规划](builder_witness.rs.md) 接入，
新增 witness peer 复用普通 add/promote。
所有状态都在局部工作副本中演算，不改宿主 region 或 store。

来源是只读 builder.go 的 prepareBuild、buildStepsWithoutJointConsensus、peerPlan、
planReplace、planReplaceLeaders、planPromotePeer、planRemovePeer、planAddPeer、
comparePlan 及各 prefer/exec 函数；builder_test.go 的 TestBuild 非 joint 场景与
TestRecord light-peer 断言作为设计依据。依用户授权，fixture 标为 source_assertion，
`go_execution=false`，没有 Go 运行差分声明。确切固定输入、输出和来源哈希见
[fixture](../../../docs/rust/schedule-execution/fixtures/task_027.json)，
[验证记录](../../../docs/rust/schedule-execution/task-027.md) 保存命令、有效数量、
失败行为变异、Cargo 槽位和源码只读审查。


源码边界：SetPeers 允许同时降级 store1 voter 并移除 store3 learner；在无显式目标
leader 的相关输入中，planReplace 可先配对 AddLearner(store1)/RemovePeer(store3)。
该步骤在真实 region 上因已存在 voter 而 CheckInProgress 返回 AlreadyVoter，GetCmd
不发命令，IsFinish=false，Operator.check 不推进。迁移保持这一 Go 源码顺序与拒绝
语义，不新增同 store 重加约束，也不宣称 Go Builder 对所有目标组合保证可达。
固定 fixture 独立冻结该反例和正常混合降级次序；枚举32种无降级 Add/Remove/Promote
目标则检查每一步真实完成语义、leader 存活和最终目标一致。之前65组合的无条件
可达性断言超出了 Go 规划契约，失败证据保留，未作为生产缺陷绕过或忽略测试。

领导者与替换评分的 canonical 实现集中在 [builder_leader.rs](builder_leader.rs.md)。
规划过程的 leader 准入使用当前 leader；prepare 阶段仍使用 origin leader。
