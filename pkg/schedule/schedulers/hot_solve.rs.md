# 热点求解与反向交换

`HotSolver::solve` 是候选求解内核。输入同轮、已通过真实 source/region/
snapshot/affinity/destination 筛选的 `HotSolveCandidate`，分别计算正向和允许的
反向净负载，复用 canonical `HotRankV1`/`HotRankV2` 及 `HotSolution`，比较后构建
真实 `Operator`。source/destination 排序复用已验证的 min/max/diff、优先级和
离散步长规则；相同候选保留已有赢家。`hot_step_rank` 对普通有限输入向零截断。
调用者提供 maxSrc/minDst/rankStep 和 top-N，不能把输入顺序当成排名。

`is_ready_to_build` 校验 peer 所属 store/region 与反向成对性，非空引用编码 nil
前提。反向搜索遵循 v2 搜索状态，拒绝同 region 和不符成员关系的 region；不同
placement 人口还须通过反向 source/target 阈值及 membership。所有 scope 来自
`HotPlacementLoads`。v1 不启用反向搜索。每轮结束按赢家更新搜索状态；构造新
solver 默认关闭搜索，宿主若保留 Go 的跨轮策略应传入上一轮结果或复用实例。

`build_operators` 根据目标 peer 存在性、source leader 身份选择真实
`PeerOperatorFactory` 的 transfer-leader/move-leader/move-peer；保留源 peer
role，使用 `RuleManager::get_rules_for_apply_region` 的规则和宿主 StoresInfo/
config/allocator，不安装运行时或自行保存状态。HIGH priority、health check
直接设置在操作器上，方向、读写类型、维度和计数器事件由
`HotOperatorDecoration` 返回给宿主 metrics 生命周期。计数器事件不在构建时发布。

反向 pair 的任一构建失败均返回 typed `BuilderError`，不暴露半个新 pair，保留
上一可执行赢家。构建可能已消耗 allocator ID，和 Go 一样不回滚 ID 分配，也不
提交操作器。没有改善、均匀负载、身份失配或空候选返回空。配置维度/peer limit
返回 `HotSolveError`。nil rules 不能绕过配置启用的规则验证；canonical builder
会拒绝缺失规则。

任务 92 的边界是求解和操作构建。候选准入使用 89/90/91 接口，读写调度接线由
94 承担；93 承担桶拆分（包括构建前尺寸拆分），95 承担 pending 生命周期。
此处没有复制筛选器、统计状态、操作步骤规划、storage 或 protobuf 模型。

Go 参考：`hot_region_solver.go` 的 solve、compareSrc/DstStore、stepRank、
isReadyToBuild、buildOperators（非 split 分支）、createOperator、decorateOperator。
源码推导与真实 Rust 正常/故障验证按用户授权验收，没有本任务 Go 运行 oracle
或运行差分。验证和边界样例见 [task-092](../../../docs/rust/schedule-execution/task-092.md)。
